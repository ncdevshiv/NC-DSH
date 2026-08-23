use svgtypes::PathSegment;

use crate::geometry::{SvgGeometryPoint, SvgGeometrySegment};

pub(crate) fn path_geometry_segments(raw: &str) -> Option<Vec<SvgGeometrySegment>> {
    let mut parser = PathGeometryBuilder {
        current: SvgGeometryPoint::new(0.0, 0.0),
        subpath_start: SvgGeometryPoint::new(0.0, 0.0),
        last_cubic_control: None,
        last_quadratic_control: None,
        previous_command: None,
        segments: Vec::new(),
    };
    for segment in svgtypes::PathParser::from(raw) {
        parser.push_path_segment(segment.ok()?);
    }
    Some(parser.segments)
}

struct PathGeometryBuilder {
    current: SvgGeometryPoint,
    subpath_start: SvgGeometryPoint,
    last_cubic_control: Option<SvgGeometryPoint>,
    last_quadratic_control: Option<SvgGeometryPoint>,
    previous_command: Option<char>,
    segments: Vec<SvgGeometrySegment>,
}

impl PathGeometryBuilder {
    fn push_path_segment(&mut self, segment: PathSegment) {
        match segment {
            PathSegment::MoveTo { abs, x, y } => {
                let target = self.target_point(abs, x, y);
                self.current = target;
                self.subpath_start = target;
                self.clear_curve_controls();
                self.previous_command = Some(if abs { 'M' } else { 'm' });
            }
            PathSegment::LineTo { abs, x, y } => {
                let target = self.target_point(abs, x, y);
                self.push_line(target);
                self.clear_curve_controls();
                self.previous_command = Some(if abs { 'L' } else { 'l' });
            }
            PathSegment::HorizontalLineTo { abs, x } => {
                let target_x = if abs { x } else { self.current.x + x };
                self.push_line(SvgGeometryPoint::new(target_x, self.current.y));
                self.clear_curve_controls();
                self.previous_command = Some(if abs { 'H' } else { 'h' });
            }
            PathSegment::VerticalLineTo { abs, y } => {
                let target_y = if abs { y } else { self.current.y + y };
                self.push_line(SvgGeometryPoint::new(self.current.x, target_y));
                self.clear_curve_controls();
                self.previous_command = Some(if abs { 'V' } else { 'v' });
            }
            PathSegment::CurveTo {
                abs,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let control1 = self.target_point(abs, x1, y1);
                let control2 = self.target_point(abs, x2, y2);
                let target = self.target_point(abs, x, y);
                self.push_cubic(control1, control2, target);
                self.last_cubic_control = Some(control2);
                self.last_quadratic_control = None;
                self.previous_command = Some(if abs { 'C' } else { 'c' });
            }
            PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
                let control1 = if self.previous_was_cubic() {
                    self.last_cubic_control
                        .map(|point| reflect_point(point, self.current))
                        .unwrap_or(self.current)
                } else {
                    self.current
                };
                let control2 = self.target_point(abs, x2, y2);
                let target = self.target_point(abs, x, y);
                self.push_cubic(control1, control2, target);
                self.last_cubic_control = Some(control2);
                self.last_quadratic_control = None;
                self.previous_command = Some(if abs { 'S' } else { 's' });
            }
            PathSegment::Quadratic { abs, x1, y1, x, y } => {
                let control = self.target_point(abs, x1, y1);
                let target = self.target_point(abs, x, y);
                self.push_quadratic(control, target);
                self.last_quadratic_control = Some(control);
                self.last_cubic_control = None;
                self.previous_command = Some(if abs { 'Q' } else { 'q' });
            }
            PathSegment::SmoothQuadratic { abs, x, y } => {
                let control = if self.previous_was_quadratic() {
                    self.last_quadratic_control
                        .map(|point| reflect_point(point, self.current))
                        .unwrap_or(self.current)
                } else {
                    self.current
                };
                let target = self.target_point(abs, x, y);
                self.push_quadratic(control, target);
                self.last_quadratic_control = Some(control);
                self.last_cubic_control = None;
                self.previous_command = Some(if abs { 'T' } else { 't' });
            }
            PathSegment::EllipticalArc {
                abs,
                rx,
                ry,
                x_axis_rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                let target = self.target_point(abs, x, y);
                self.push_arc(rx, ry, x_axis_rotation, large_arc, sweep, target);
                self.clear_curve_controls();
                self.previous_command = Some(if abs { 'A' } else { 'a' });
            }
            PathSegment::ClosePath { abs } => {
                self.push_line(self.subpath_start);
                self.current = self.subpath_start;
                self.clear_curve_controls();
                self.previous_command = Some(if abs { 'Z' } else { 'z' });
            }
        }
    }

    fn target_point(&self, absolute: bool, x: f64, y: f64) -> SvgGeometryPoint {
        if absolute {
            SvgGeometryPoint::new(x, y)
        } else {
            SvgGeometryPoint::new(self.current.x + x, self.current.y + y)
        }
    }

    fn push_line(&mut self, target: SvgGeometryPoint) {
        self.segments
            .push(SvgGeometrySegment::new(self.current, target));
        self.current = target;
    }

    fn push_arc(
        &mut self,
        rx: f64,
        ry: f64,
        x_axis_rotation: f64,
        large_arc: bool,
        sweep: bool,
        target: SvgGeometryPoint,
    ) {
        let Some(arc) = SvgArcCenterParameters::from_endpoint(
            self.current,
            target,
            rx,
            ry,
            x_axis_rotation,
            large_arc,
            sweep,
        ) else {
            self.push_line(target);
            return;
        };
        let steps =
            ((arc.delta_theta.abs() / (std::f64::consts::PI / 16.0)).ceil() as usize).clamp(8, 64);
        let mut previous = self.current;
        for index in 1..=steps {
            let point = arc.point_at(index as f64 / steps as f64);
            self.segments.push(SvgGeometrySegment::new(previous, point));
            previous = point;
        }
        self.current = target;
    }

    fn push_cubic(
        &mut self,
        control1: SvgGeometryPoint,
        control2: SvgGeometryPoint,
        target: SvgGeometryPoint,
    ) {
        let start = self.current;
        self.push_sampled_curve(24, |t| cubic_point(start, control1, control2, target, t));
        self.current = target;
    }

    fn push_quadratic(&mut self, control: SvgGeometryPoint, target: SvgGeometryPoint) {
        let start = self.current;
        self.push_sampled_curve(20, |t| quadratic_point(start, control, target, t));
        self.current = target;
    }

    fn push_sampled_curve(&mut self, steps: usize, point_at_t: impl Fn(f64) -> SvgGeometryPoint) {
        let mut previous = self.current;
        for index in 1..=steps {
            let point = point_at_t(index as f64 / steps as f64);
            self.segments.push(SvgGeometrySegment::new(previous, point));
            previous = point;
        }
    }

    fn clear_curve_controls(&mut self) {
        self.last_cubic_control = None;
        self.last_quadratic_control = None;
    }

    fn previous_was_cubic(&self) -> bool {
        matches!(self.previous_command, Some('C' | 'c' | 'S' | 's'))
    }

    fn previous_was_quadratic(&self) -> bool {
        matches!(self.previous_command, Some('Q' | 'q' | 'T' | 't'))
    }
}

struct SvgArcCenterParameters {
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    phi: f64,
    theta1: f64,
    delta_theta: f64,
}

impl SvgArcCenterParameters {
    fn from_endpoint(
        start: SvgGeometryPoint,
        end: SvgGeometryPoint,
        rx: f64,
        ry: f64,
        x_axis_rotation: f64,
        large_arc: bool,
        sweep: bool,
    ) -> Option<Self> {
        if (start.x == end.x && start.y == end.y) || rx == 0.0 || ry == 0.0 {
            return None;
        }

        let mut rx = rx.abs();
        let mut ry = ry.abs();
        let phi = x_axis_rotation.to_radians();
        let cos_phi = phi.cos();
        let sin_phi = phi.sin();
        let dx2 = (start.x - end.x) / 2.0;
        let dy2 = (start.y - end.y) / 2.0;
        let x1_prime = cos_phi * dx2 + sin_phi * dy2;
        let y1_prime = -sin_phi * dx2 + cos_phi * dy2;

        let radii_scale = x1_prime.powi(2) / rx.powi(2) + y1_prime.powi(2) / ry.powi(2);
        if radii_scale > 1.0 {
            let scale = radii_scale.sqrt();
            rx *= scale;
            ry *= scale;
        }

        let rx2 = rx.powi(2);
        let ry2 = ry.powi(2);
        let x1_prime2 = x1_prime.powi(2);
        let y1_prime2 = y1_prime.powi(2);
        let denominator = rx2 * y1_prime2 + ry2 * x1_prime2;
        if denominator == 0.0 {
            return None;
        }
        let numerator = rx2 * ry2 - rx2 * y1_prime2 - ry2 * x1_prime2;
        let sign = if large_arc == sweep { -1.0 } else { 1.0 };
        let coefficient = sign * (numerator / denominator).max(0.0).sqrt();
        let cx_prime = coefficient * (rx * y1_prime / ry);
        let cy_prime = coefficient * -(ry * x1_prime / rx);
        let cx = cos_phi * cx_prime - sin_phi * cy_prime + (start.x + end.x) / 2.0;
        let cy = sin_phi * cx_prime + cos_phi * cy_prime + (start.y + end.y) / 2.0;

        let start_vector =
            SvgGeometryPoint::new((x1_prime - cx_prime) / rx, (y1_prime - cy_prime) / ry);
        let end_vector =
            SvgGeometryPoint::new((-x1_prime - cx_prime) / rx, (-y1_prime - cy_prime) / ry);
        let theta1 = vector_angle(SvgGeometryPoint::new(1.0, 0.0), start_vector);
        let mut delta_theta = vector_angle(start_vector, end_vector);
        if !sweep && delta_theta > 0.0 {
            delta_theta -= std::f64::consts::TAU;
        } else if sweep && delta_theta < 0.0 {
            delta_theta += std::f64::consts::TAU;
        }

        Some(Self {
            cx,
            cy,
            rx,
            ry,
            phi,
            theta1,
            delta_theta,
        })
    }

    fn point_at(&self, t: f64) -> SvgGeometryPoint {
        let theta = self.theta1 + self.delta_theta * t;
        let cos_phi = self.phi.cos();
        let sin_phi = self.phi.sin();
        let x = self.rx * theta.cos();
        let y = self.ry * theta.sin();
        SvgGeometryPoint::new(
            self.cx + cos_phi * x - sin_phi * y,
            self.cy + sin_phi * x + cos_phi * y,
        )
    }
}

fn vector_angle(u: SvgGeometryPoint, v: SvgGeometryPoint) -> f64 {
    let dot = u.x * v.x + u.y * v.y;
    let cross = u.x * v.y - u.y * v.x;
    cross.atan2(dot)
}

fn reflect_point(point: SvgGeometryPoint, origin: SvgGeometryPoint) -> SvgGeometryPoint {
    SvgGeometryPoint::new(origin.x * 2.0 - point.x, origin.y * 2.0 - point.y)
}

fn cubic_point(
    start: SvgGeometryPoint,
    control1: SvgGeometryPoint,
    control2: SvgGeometryPoint,
    end: SvgGeometryPoint,
    t: f64,
) -> SvgGeometryPoint {
    let mt = 1.0 - t;
    SvgGeometryPoint::new(
        mt.powi(3) * start.x
            + 3.0 * mt.powi(2) * t * control1.x
            + 3.0 * mt * t.powi(2) * control2.x
            + t.powi(3) * end.x,
        mt.powi(3) * start.y
            + 3.0 * mt.powi(2) * t * control1.y
            + 3.0 * mt * t.powi(2) * control2.y
            + t.powi(3) * end.y,
    )
}

fn quadratic_point(
    start: SvgGeometryPoint,
    control: SvgGeometryPoint,
    end: SvgGeometryPoint,
    t: f64,
) -> SvgGeometryPoint {
    let mt = 1.0 - t;
    SvgGeometryPoint::new(
        mt.powi(2) * start.x + 2.0 * mt * t * control.x + t.powi(2) * end.x,
        mt.powi(2) * start.y + 2.0 * mt * t * control.y + t.powi(2) * end.y,
    )
}
