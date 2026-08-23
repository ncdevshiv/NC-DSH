//! DOM-neutral `clip-path` projection.
//!
//! The basic-shape conversion is a direct port of
//! `blitz-paint/src/render/clip_path.rs` at Blitz 5081c658. The concrete
//! `CssBox` is replaced by [`super::geometry::BoxAreas`], and Kurbo paths are serialized
//! into the owned paint schema before the layout pass returns.

use kurbo::{Affine, BezPath, Circle, Ellipse, PathEl, Point, Rect, Shape, SvgArc, Vec2};
use style::values::computed::basic_shape::{BasicShape, ClipPath};
use style::values::computed::{Angle, CSSPixelLength, LengthPercentage};
use style::values::generics::basic_shape::{
    ArcSize, ArcSweep, AxisEndPoint, AxisPosition, CommandEndPoint, ControlPoint, ControlReference,
    GenericBasicShape, GenericPathOrShapeFunction, GenericShapeCommand, GenericShapeRadius,
    ShapeBox, ShapeGeometryBox,
};
use style::values::generics::position::{GenericPosition, GenericPositionOrAuto};

use crate::{
    LayoutPoint, LayoutRect, PaintPath, PaintPathElement, PaintShape, ResolvedLayoutStyle,
};

use super::geometry::BoxAreas;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClipPathUnsupported {
    UrlReference,
}

pub(super) fn project_clip_path(
    style: &ResolvedLayoutStyle,
    areas: BoxAreas,
) -> Result<Option<PaintShape>, ClipPathUnsupported> {
    let Some(computed) = style.stylo_computed_values() else {
        return Ok(None);
    };
    match computed.clone_clip_path() {
        ClipPath::None => Ok(None),
        ClipPath::Url(_) => Err(ClipPathUnsupported::UrlReference),
        ClipPath::Shape(shape, geometry_box) => {
            let reference_box = resolve_geometry_box(areas, &geometry_box);
            basic_shape_to_path(&shape, reference_box).map(|path| path.map(owned_shape))
        }
        ClipPath::Box(geometry_box) => {
            let reference_box = resolve_geometry_box(areas, &geometry_box);
            Ok(Some(owned_shape(Rect::from(reference_box).into_path(0.1))))
        }
    }
}

fn resolve_geometry_box(areas: BoxAreas, geometry_box: &ShapeGeometryBox) -> ReferenceBox {
    let rect = match geometry_box {
        ShapeGeometryBox::ElementDependent
        | ShapeGeometryBox::StrokeBox
        | ShapeGeometryBox::ViewBox
        | ShapeGeometryBox::ShapeBox(ShapeBox::BorderBox) => areas.border_rect,
        ShapeGeometryBox::ShapeBox(ShapeBox::PaddingBox) => areas.padding_rect,
        ShapeGeometryBox::FillBox | ShapeGeometryBox::ShapeBox(ShapeBox::ContentBox) => {
            areas.content_rect
        }
        ShapeGeometryBox::ShapeBox(ShapeBox::MarginBox) => areas.margin_rect,
    };
    ReferenceBox::from(rect)
}

fn basic_shape_to_path(
    shape: &BasicShape,
    reference_box: ReferenceBox,
) -> Result<Option<BezPath>, ClipPathUnsupported> {
    let ReferenceBox {
        x: ox,
        y: oy,
        width: w,
        height: h,
    } = reference_box;
    Ok(match shape {
        GenericBasicShape::Circle(circle) => {
            let (cx, cy) = resolve_position(&circle.position, w, h, ox, oy);
            let r = resolve_shape_radius(&circle.radius, w, h, cx - ox, cy - oy);
            Some(Circle::new(Point::new(cx, cy), r).into_path(0.1))
        }
        GenericBasicShape::Ellipse(ellipse) => {
            let (cx, cy) = resolve_position(&ellipse.position, w, h, ox, oy);
            let rx = resolve_shape_radius(&ellipse.semiaxis_x, w, h, cx - ox, cy - oy);
            let ry = resolve_shape_radius(&ellipse.semiaxis_y, h, w, cy - oy, cx - ox);
            Some(Ellipse::new(Point::new(cx, cy), (rx, ry), 0.0).into_path(0.1))
        }
        GenericBasicShape::Polygon(polygon) => {
            if polygon.coordinates.is_empty() {
                return Ok(None);
            }
            // Blitz currently relies on the backend's non-zero clip fill even
            // when the parsed polygon carries an alternate fill rule.
            let _fill = &polygon.fill;
            let mut path = BezPath::new();
            for (index, coordinate) in polygon.coordinates.iter().enumerate() {
                let point = Point::new(
                    ox + resolve_lp(&coordinate.0, w),
                    oy + resolve_lp(&coordinate.1, h),
                );
                if index == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
            path.close_path();
            Some(path)
        }
        GenericBasicShape::Rect(inset_rect) => {
            let top = resolve_lp(&inset_rect.rect.0, h);
            let right = resolve_lp(&inset_rect.rect.1, w);
            let bottom = resolve_lp(&inset_rect.rect.2, h);
            let left = resolve_lp(&inset_rect.rect.3, w);
            let x0 = ox + left;
            let y0 = oy + top;
            let x1 = ox + w - right;
            let y1 = oy + h - bottom;
            (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1, y1).into_path(0.1))
        }
        GenericBasicShape::PathOrShape(path_or_shape) => match path_or_shape {
            GenericPathOrShapeFunction::Path(path) => svg_path_to_bezpath(
                path.commands(),
                w,
                h,
                |value| f64::from(*value),
                |value| f64::from(*value),
                |value| f64::from(*value),
            )
            .map(|mut path| {
                path.apply_affine(Affine::translate((ox, oy)));
                path
            }),
            GenericPathOrShapeFunction::Shape(shape) => svg_path_to_bezpath(
                &shape.commands,
                w,
                h,
                |value: &LengthPercentage| {
                    f64::from(value.resolve(CSSPixelLength::new(w as f32)).px())
                },
                |value: &LengthPercentage| {
                    f64::from(value.resolve(CSSPixelLength::new(h as f32)).px())
                },
                |value: &Angle| f64::from(value.degrees()),
            )
            .map(|mut path| {
                path.apply_affine(Affine::translate((ox, oy)));
                path
            }),
        },
    })
}

#[derive(Clone, Copy)]
struct ReferenceBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl From<LayoutRect> for ReferenceBox {
    fn from(rect: LayoutRect) -> Self {
        Self {
            x: f64::from(rect.x),
            y: f64::from(rect.y),
            width: f64::from(rect.width),
            height: f64::from(rect.height),
        }
    }
}

impl From<ReferenceBox> for Rect {
    fn from(reference_box: ReferenceBox) -> Self {
        Self::new(
            reference_box.x,
            reference_box.y,
            reference_box.x + reference_box.width,
            reference_box.y + reference_box.height,
        )
    }
}

fn resolve_lp(value: &LengthPercentage, basis: f64) -> f64 {
    f64::from(value.resolve(CSSPixelLength::new(basis as f32)).px())
}

fn resolve_position(
    position: &GenericPositionOrAuto<style::values::computed::Position>,
    width: f64,
    height: f64,
    origin_x: f64,
    origin_y: f64,
) -> (f64, f64) {
    match position {
        GenericPositionOrAuto::Auto => (origin_x + width / 2.0, origin_y + height / 2.0),
        GenericPositionOrAuto::Position(position) => (
            origin_x + resolve_lp(&position.horizontal, width),
            origin_y + resolve_lp(&position.vertical, height),
        ),
    }
}

fn resolve_shape_radius(
    radius: &GenericShapeRadius<LengthPercentage>,
    primary_size: f64,
    secondary_size: f64,
    center_offset_primary: f64,
    center_offset_secondary: f64,
) -> f64 {
    match radius {
        GenericShapeRadius::Length(length) => resolve_lp(&length.0, primary_size),
        GenericShapeRadius::ClosestSide => center_offset_primary
            .min(primary_size - center_offset_primary)
            .min(center_offset_secondary)
            .min(secondary_size - center_offset_secondary)
            .max(0.0),
        GenericShapeRadius::FarthestSide => center_offset_primary
            .max(primary_size - center_offset_primary)
            .max(center_offset_secondary)
            .max(secondary_size - center_offset_secondary),
        GenericShapeRadius::FarthestCorner => {
            let primary = center_offset_primary.max(primary_size - center_offset_primary);
            let secondary = center_offset_secondary.max(secondary_size - center_offset_secondary);
            primary.hypot(secondary)
        }
        GenericShapeRadius::ClosestCorner => {
            let primary = center_offset_primary
                .min(primary_size - center_offset_primary)
                .max(0.0);
            let secondary = center_offset_secondary
                .min(secondary_size - center_offset_secondary)
                .max(0.0);
            primary.hypot(secondary)
        }
    }
}

type GenericPathCommand<Angle, Number> =
    GenericShapeCommand<Angle, GenericPosition<Number, Number>, Number>;

fn svg_path_to_bezpath<AngleValue: Copy, Number>(
    commands: &[GenericPathCommand<AngleValue, Number>],
    width: f64,
    height: f64,
    resolve_x: impl Fn(&Number) -> f64,
    resolve_y: impl Fn(&Number) -> f64,
    resolve_angle: impl Fn(&AngleValue) -> f64,
) -> Option<BezPath> {
    if commands.is_empty() {
        return None;
    }

    let mut path = BezPath::new();
    let mut current = Point::ZERO;
    let mut subpath_start = current;
    let mut last_control = None;

    for command in commands {
        match command {
            GenericShapeCommand::Close => {
                path.close_path();
                current = subpath_start;
                last_control = None;
            }
            GenericShapeCommand::Move { point } => {
                let point = resolve_endpoint(point, current, &resolve_x, &resolve_y);
                path.move_to(point);
                current = point;
                subpath_start = point;
                last_control = None;
            }
            GenericShapeCommand::Line { point } => {
                let point = resolve_endpoint(point, current, &resolve_x, &resolve_y);
                path.line_to(point);
                current = point;
                last_control = None;
            }
            GenericShapeCommand::HLine { x } => {
                current.x = resolve_axis_endpoint(x, current.x, width, &resolve_x);
                path.line_to(current);
                last_control = None;
            }
            GenericShapeCommand::VLine { y } => {
                current.y = resolve_axis_endpoint(y, current.y, height, &resolve_y);
                path.line_to(current);
                last_control = None;
            }
            GenericShapeCommand::CubicCurve {
                point,
                control1,
                control2,
            } => {
                let point = resolve_endpoint(point, current, &resolve_x, &resolve_y);
                let control1 =
                    resolve_control_point(control1, current, point, &resolve_x, &resolve_y);
                let control2 =
                    resolve_control_point(control2, current, point, &resolve_x, &resolve_y);
                path.curve_to(control1, control2, point);
                last_control = Some(control2);
                current = point;
            }
            GenericShapeCommand::QuadCurve { point, control1 } => {
                let point = resolve_endpoint(point, current, &resolve_x, &resolve_y);
                let control =
                    resolve_control_point(control1, current, point, &resolve_x, &resolve_y);
                path.quad_to(control, point);
                last_control = Some(control);
                current = point;
            }
            GenericShapeCommand::SmoothCubic { point, control2 } => {
                let point = resolve_endpoint(point, current, &resolve_x, &resolve_y);
                let control2 =
                    resolve_control_point(control2, current, point, &resolve_x, &resolve_y);
                let control1 = reflect_point(last_control, current);
                path.curve_to(control1, control2, point);
                last_control = Some(control2);
                current = point;
            }
            GenericShapeCommand::SmoothQuad { point } => {
                let point = resolve_endpoint(point, current, &resolve_x, &resolve_y);
                let control = reflect_point(last_control, current);
                path.quad_to(control, point);
                last_control = Some(control);
                current = point;
            }
            GenericShapeCommand::Arc {
                point,
                radii,
                arc_sweep,
                arc_size,
                rotate,
            } => {
                let point = resolve_endpoint(point, current, &resolve_x, &resolve_y);
                let arc = kurbo::Arc::from_svg_arc(&SvgArc {
                    from: current,
                    to: point,
                    radii: Vec2::new(
                        resolve_x(&radii.rx),
                        resolve_y(radii.ry.as_ref().unwrap_or(&radii.rx)),
                    ),
                    x_rotation: resolve_angle(rotate),
                    large_arc: matches!(arc_size, ArcSize::Large),
                    sweep: matches!(arc_sweep, ArcSweep::Ccw),
                })?;
                path.extend(arc.append_iter(0.1));
                last_control = None;
                current = point;
            }
        }
    }
    Some(path)
}

fn resolve_endpoint<Number>(
    endpoint: &CommandEndPoint<GenericPosition<Number, Number>, Number>,
    current: Point,
    resolve_x: impl Fn(&Number) -> f64,
    resolve_y: impl Fn(&Number) -> f64,
) -> Point {
    match endpoint {
        CommandEndPoint::ToPosition(position) => Point::new(
            resolve_x(&position.horizontal),
            resolve_y(&position.vertical),
        ),
        CommandEndPoint::ByCoordinate(coordinate) => Point::new(
            current.x + resolve_x(&coordinate.x),
            current.y + resolve_y(&coordinate.y),
        ),
    }
}

fn resolve_control_point<Number>(
    control_point: &ControlPoint<GenericPosition<Number, Number>, Number>,
    current: Point,
    end: Point,
    resolve_x: impl Fn(&Number) -> f64,
    resolve_y: impl Fn(&Number) -> f64,
) -> Point {
    match control_point {
        ControlPoint::Absolute(position) => Point::new(
            resolve_x(&position.horizontal),
            resolve_y(&position.vertical),
        ),
        ControlPoint::Relative(relative) => {
            let base = match relative.reference {
                ControlReference::Start => current,
                ControlReference::End => end,
                ControlReference::Origin => Point::ZERO,
            };
            Point::new(
                base.x + resolve_x(&relative.coord.x),
                base.y + resolve_y(&relative.coord.y),
            )
        }
    }
}

fn reflect_point(last_control: Option<Point>, current: Point) -> Point {
    last_control.map_or(current, |control| {
        Point::new(2.0 * current.x - control.x, 2.0 * current.y - control.y)
    })
}

fn resolve_axis_endpoint<Number>(
    endpoint: &AxisEndPoint<Number>,
    current: f64,
    basis: f64,
    resolve: impl Fn(&Number) -> f64,
) -> f64 {
    use style::values::generics::basic_shape::AxisPositionKeyword;
    match endpoint {
        AxisEndPoint::ToPosition(AxisPosition::LengthPercent(value)) => resolve(value),
        AxisEndPoint::ToPosition(AxisPosition::Keyword(keyword)) => match keyword {
            AxisPositionKeyword::Left | AxisPositionKeyword::XStart => 0.0,
            AxisPositionKeyword::Right | AxisPositionKeyword::XEnd => basis,
            AxisPositionKeyword::Top | AxisPositionKeyword::YStart => 0.0,
            AxisPositionKeyword::Bottom | AxisPositionKeyword::YEnd => basis,
            AxisPositionKeyword::Center => basis / 2.0,
        },
        AxisEndPoint::ByCoordinate(value) => current + resolve(value),
    }
}

fn owned_shape(path: BezPath) -> PaintShape {
    let bounds = path.bounding_box();
    PaintShape::Path(PaintPath {
        elements: path
            .elements()
            .iter()
            .map(|element| match *element {
                PathEl::MoveTo(point) => PaintPathElement::MoveTo(owned_point(point)),
                PathEl::LineTo(point) => PaintPathElement::LineTo(owned_point(point)),
                PathEl::QuadTo(control, point) => {
                    PaintPathElement::QuadTo(owned_point(control), owned_point(point))
                }
                PathEl::CurveTo(control1, control2, point) => PaintPathElement::CubicTo(
                    owned_point(control1),
                    owned_point(control2),
                    owned_point(point),
                ),
                PathEl::ClosePath => PaintPathElement::Close,
            })
            .collect(),
        bounds: LayoutRect::new(
            bounds.x0 as f32,
            bounds.y0 as f32,
            bounds.width().max(0.0) as f32,
            bounds.height().max(0.0) as f32,
        ),
    })
}

fn owned_point(point: Point) -> LayoutPoint {
    LayoutPoint::new(point.x as f32, point.y as f32)
}
