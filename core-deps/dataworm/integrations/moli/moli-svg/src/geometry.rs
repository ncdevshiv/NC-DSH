use crate::helpers::svg_number_list;
use crate::path::path_geometry_segments;

#[derive(Clone, Copy, Debug)]
pub struct SvgGeometryPoint {
    pub x: f64,
    pub y: f64,
}

impl SvgGeometryPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SvgGeometrySegment {
    start: SvgGeometryPoint,
    end: SvgGeometryPoint,
}

impl SvgGeometrySegment {
    pub(crate) fn new(start: SvgGeometryPoint, end: SvgGeometryPoint) -> Self {
        Self { start, end }
    }

    pub fn length(&self) -> f64 {
        (self.end.x - self.start.x).hypot(self.end.y - self.start.y)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SvgGeometryBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub enum SvgGeometryElement {
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
    },
    Ellipse {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    Path {
        d: String,
    },
    Polygon {
        points: String,
    },
    Polyline {
        points: String,
    },
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        rx: f64,
        ry: f64,
    },
}

pub fn segments_for_element(element: SvgGeometryElement) -> Vec<SvgGeometrySegment> {
    match element {
        SvgGeometryElement::Circle { cx, cy, r } => circle_segments(cx, cy, r),
        SvgGeometryElement::Ellipse { cx, cy, rx, ry } => ellipse_segments(cx, cy, rx, ry),
        SvgGeometryElement::Line { x1, y1, x2, y2 } => {
            vec![SvgGeometrySegment::new(
                SvgGeometryPoint::new(x1, y1),
                SvgGeometryPoint::new(x2, y2),
            )]
        }
        SvgGeometryElement::Path { d } => path_geometry_segments(&d).unwrap_or_default(),
        SvgGeometryElement::Polygon { points } => {
            poly_points_geometry_segments(&points, true).unwrap_or_default()
        }
        SvgGeometryElement::Polyline { points } => {
            poly_points_geometry_segments(&points, false).unwrap_or_default()
        }
        SvgGeometryElement::Rect {
            x,
            y,
            width,
            height,
            rx,
            ry,
        } => rect_segments(x, y, width, height, rx, ry),
    }
}

pub fn bounding_box_for_segments(segments: &[SvgGeometrySegment]) -> Option<SvgGeometryBox> {
    let first = segments.first()?;
    let mut min_x = first.start.x.min(first.end.x);
    let mut min_y = first.start.y.min(first.end.y);
    let mut max_x = first.start.x.max(first.end.x);
    let mut max_y = first.start.y.max(first.end.y);
    for segment in &segments[1..] {
        min_x = min_x.min(segment.start.x).min(segment.end.x);
        min_y = min_y.min(segment.start.y).min(segment.end.y);
        max_x = max_x.max(segment.start.x).max(segment.end.x);
        max_y = max_y.max(segment.start.y).max(segment.end.y);
    }
    Some(SvgGeometryBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

pub fn is_point_in_fill(element: &SvgGeometryElement, point: SvgGeometryPoint) -> bool {
    match element {
        SvgGeometryElement::Circle { cx, cy, r } => {
            *r > 0.0 && (point.x - cx).hypot(point.y - cy) <= *r
        }
        SvgGeometryElement::Ellipse { cx, cy, rx, ry } => {
            if *rx <= 0.0 || *ry <= 0.0 {
                return false;
            }
            let x = (point.x - cx) / rx;
            let y = (point.y - cy) / ry;
            x * x + y * y <= 1.0
        }
        SvgGeometryElement::Line { .. } => false,
        SvgGeometryElement::Path { d } => path_geometry_segments(d)
            .is_some_and(|segments| is_point_in_closed_segments(&segments, point)),
        SvgGeometryElement::Polygon { points } => poly_points_geometry_segments(points, true)
            .is_some_and(|segments| is_point_in_closed_segments(&segments, point)),
        SvgGeometryElement::Polyline { points } => poly_points_geometry_segments(points, false)
            .is_some_and(|segments| is_point_in_closed_segments(&segments, point)),
        SvgGeometryElement::Rect {
            x,
            y,
            width,
            height,
            rx,
            ry,
        } => {
            if *width <= 0.0
                || *height <= 0.0
                || point.x < *x
                || point.x > *x + *width
                || point.y < *y
                || point.y > *y + *height
            {
                return false;
            }
            let (rx, ry) = normalized_rect_radii(*width, *height, *rx, *ry);
            if rx == 0.0 || ry == 0.0 {
                return true;
            }
            let corner_center_x = if point.x < *x + rx {
                *x + rx
            } else if point.x > *x + *width - rx {
                *x + *width - rx
            } else {
                return true;
            };
            let corner_center_y = if point.y < *y + ry {
                *y + ry
            } else if point.y > *y + *height - ry {
                *y + *height - ry
            } else {
                return true;
            };
            let dx = (point.x - corner_center_x) / rx;
            let dy = (point.y - corner_center_y) / ry;
            dx * dx + dy * dy <= 1.0
        }
    }
}

pub fn point_at_length(segments: &[SvgGeometrySegment], distance: f64) -> SvgGeometryPoint {
    let Some(first) = segments.first() else {
        return SvgGeometryPoint::new(0.0, 0.0);
    };
    if distance <= 0.0 {
        return first.start;
    }
    let mut remaining = distance;
    for segment in segments {
        let length = segment.length();
        if length == 0.0 {
            continue;
        }
        if remaining <= length {
            let t = remaining / length;
            return SvgGeometryPoint::new(
                segment.start.x + (segment.end.x - segment.start.x) * t,
                segment.start.y + (segment.end.y - segment.start.y) * t,
            );
        }
        remaining -= length;
    }
    segments
        .last()
        .map(|segment| segment.end)
        .unwrap_or(SvgGeometryPoint::new(0.0, 0.0))
}

fn circle_segments(cx: f64, cy: f64, r: f64) -> Vec<SvgGeometrySegment> {
    if r <= 0.0 {
        return Vec::new();
    }
    sampled_closed_parametric_segments(64, |theta| {
        SvgGeometryPoint::new(cx + r * theta.cos(), cy + r * theta.sin())
    })
}

fn ellipse_segments(cx: f64, cy: f64, rx: f64, ry: f64) -> Vec<SvgGeometrySegment> {
    if rx <= 0.0 || ry <= 0.0 {
        return Vec::new();
    }
    sampled_closed_parametric_segments(96, |theta| {
        SvgGeometryPoint::new(cx + rx * theta.cos(), cy + ry * theta.sin())
    })
}

fn sampled_closed_parametric_segments(
    steps: usize,
    point_at_angle: impl Fn(f64) -> SvgGeometryPoint,
) -> Vec<SvgGeometrySegment> {
    let mut points = Vec::with_capacity(steps);
    for index in 0..steps {
        let theta = (index as f64 / steps as f64) * std::f64::consts::TAU;
        points.push(point_at_angle(theta));
    }
    segments_from_points(&points, true)
}

fn rect_segments(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rx: f64,
    ry: f64,
) -> Vec<SvgGeometrySegment> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    let (rx, ry) = normalized_rect_radii(width, height, rx, ry);
    if rx > 0.0 && ry > 0.0 {
        return rounded_rect_segments(x, y, width, height, rx, ry);
    }
    let top_left = SvgGeometryPoint::new(x, y);
    let top_right = SvgGeometryPoint::new(x + width, y);
    let bottom_right = SvgGeometryPoint::new(x + width, y + height);
    let bottom_left = SvgGeometryPoint::new(x, y + height);
    vec![
        SvgGeometrySegment::new(top_left, top_right),
        SvgGeometrySegment::new(top_right, bottom_right),
        SvgGeometrySegment::new(bottom_right, bottom_left),
        SvgGeometrySegment::new(bottom_left, top_left),
    ]
}

fn normalized_rect_radii(width: f64, height: f64, rx: f64, ry: f64) -> (f64, f64) {
    if rx <= 0.0 || ry <= 0.0 {
        return (0.0, 0.0);
    }
    (rx.min(width / 2.0), ry.min(height / 2.0))
}

fn rounded_rect_segments(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rx: f64,
    ry: f64,
) -> Vec<SvgGeometrySegment> {
    let mut points = vec![
        SvgGeometryPoint::new(x + rx, y),
        SvgGeometryPoint::new(x + width - rx, y),
    ];
    push_rounded_rect_corner_points(&mut points, x + width - rx, y + ry, -90.0, 0.0, rx, ry);
    points.push(SvgGeometryPoint::new(x + width, y + height - ry));
    push_rounded_rect_corner_points(
        &mut points,
        x + width - rx,
        y + height - ry,
        0.0,
        90.0,
        rx,
        ry,
    );
    points.push(SvgGeometryPoint::new(x + rx, y + height));
    push_rounded_rect_corner_points(&mut points, x + rx, y + height - ry, 90.0, 180.0, rx, ry);
    points.push(SvgGeometryPoint::new(x, y + ry));
    push_rounded_rect_corner_points(&mut points, x + rx, y + ry, 180.0, 270.0, rx, ry);
    segments_from_points(&points, true)
}

fn push_rounded_rect_corner_points(
    points: &mut Vec<SvgGeometryPoint>,
    cx: f64,
    cy: f64,
    start_degrees: f64,
    end_degrees: f64,
    rx: f64,
    ry: f64,
) {
    const ROUNDED_RECT_ARC_STEPS: usize = 8;
    for step in 1..=ROUNDED_RECT_ARC_STEPS {
        let t = step as f64 / ROUNDED_RECT_ARC_STEPS as f64;
        let angle = (start_degrees + (end_degrees - start_degrees) * t).to_radians();
        points.push(SvgGeometryPoint::new(
            cx + rx * angle.cos(),
            cy + ry * angle.sin(),
        ));
    }
}

pub(crate) fn poly_points_geometry_segments(
    raw: &str,
    close: bool,
) -> Option<Vec<SvgGeometrySegment>> {
    let coordinates = svg_number_list(raw)?;
    if coordinates.len() < 4 || !coordinates.len().is_multiple_of(2) {
        return None;
    }
    let mut points = Vec::with_capacity(coordinates.len() / 2);
    for pair in coordinates.chunks_exact(2) {
        points.push(SvgGeometryPoint::new(pair[0], pair[1]));
    }
    Some(segments_from_points(&points, close))
}

fn is_point_in_closed_segments(segments: &[SvgGeometrySegment], point: SvgGeometryPoint) -> bool {
    if segments.is_empty() {
        return false;
    }
    if segments
        .iter()
        .any(|segment| point_is_on_segment(point, segment))
    {
        return true;
    }

    let mut inside = false;
    for segment in segments {
        toggle_ray_crossing(segment, point, &mut inside);
    }
    if let Some(closing_segment) = closing_segment_for_segments(segments) {
        toggle_ray_crossing(&closing_segment, point, &mut inside);
    }
    inside
}

fn closing_segment_for_segments(segments: &[SvgGeometrySegment]) -> Option<SvgGeometrySegment> {
    let first = segments.first()?;
    let last = segments.last()?;
    if points_are_close(last.end, first.start) {
        None
    } else {
        Some(SvgGeometrySegment::new(last.end, first.start))
    }
}

fn toggle_ray_crossing(segment: &SvgGeometrySegment, point: SvgGeometryPoint, inside: &mut bool) {
    let y1 = segment.start.y;
    let y2 = segment.end.y;
    if (y1 > point.y) == (y2 > point.y) {
        return;
    }
    let x_intersection =
        segment.start.x + (point.y - y1) * (segment.end.x - segment.start.x) / (y2 - y1);
    if point.x < x_intersection {
        *inside = !*inside;
    }
}

fn point_is_on_segment(point: SvgGeometryPoint, segment: &SvgGeometrySegment) -> bool {
    const EPSILON: f64 = 1e-9;
    let dx = segment.end.x - segment.start.x;
    let dy = segment.end.y - segment.start.y;
    let cross = (point.x - segment.start.x) * dy - (point.y - segment.start.y) * dx;
    if cross.abs() > EPSILON {
        return false;
    }
    let dot = (point.x - segment.start.x) * dx + (point.y - segment.start.y) * dy;
    if dot < -EPSILON {
        return false;
    }
    dot <= dx * dx + dy * dy + EPSILON
}

fn points_are_close(a: SvgGeometryPoint, b: SvgGeometryPoint) -> bool {
    const EPSILON: f64 = 1e-9;
    (a.x - b.x).abs() <= EPSILON && (a.y - b.y).abs() <= EPSILON
}

fn segments_from_points(points: &[SvgGeometryPoint], close: bool) -> Vec<SvgGeometrySegment> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut segments = points
        .windows(2)
        .map(|pair| SvgGeometrySegment::new(pair[0], pair[1]))
        .collect::<Vec<_>>();
    if close && let Some(last) = points.last().copied() {
        segments.push(SvgGeometrySegment::new(last, points[0]));
    }
    segments
}
