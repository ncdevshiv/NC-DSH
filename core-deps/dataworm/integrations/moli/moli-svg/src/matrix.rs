#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgMatrixComponents {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl SvgMatrixComponents {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn translate(x: f64, y: f64) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::identity()
        }
    }

    pub fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::identity()
        }
    }

    pub fn rotate(angle: f64) -> Self {
        let radians = angle.to_radians();
        let cos = radians.cos();
        let sin = radians.sin();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn rotate_around(angle: f64, cx: f64, cy: f64) -> Self {
        Self::translate(cx, cy)
            .multiply(Self::rotate(angle))
            .multiply(Self::translate(-cx, -cy))
    }

    pub fn skew_x(angle: f64) -> Self {
        Self {
            c: angle.to_radians().tan(),
            ..Self::identity()
        }
    }

    pub fn skew_y(angle: f64) -> Self {
        Self {
            b: angle.to_radians().tan(),
            ..Self::identity()
        }
    }

    pub fn multiply(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    pub fn then_translate(self, x: f64, y: f64) -> Self {
        self.multiply(Self::translate(x, y))
    }

    pub fn then_scale(self, factor: f64) -> Self {
        self.then_scale_non_uniform(factor, factor)
    }

    pub fn then_scale_non_uniform(self, x: f64, y: f64) -> Self {
        self.multiply(Self::scale(x, y))
    }

    pub fn then_rotate(self, angle: f64) -> Self {
        self.multiply(Self::rotate(angle))
    }

    pub fn then_rotate_from_vector(self, x: f64, y: f64) -> Option<Self> {
        if x == 0.0 || y == 0.0 {
            return None;
        }
        Some(self.then_rotate(y.atan2(x).to_degrees()))
    }

    pub fn then_flip_x(self) -> Self {
        self.then_scale_non_uniform(-1.0, 1.0)
    }

    pub fn then_flip_y(self) -> Self {
        self.then_scale_non_uniform(1.0, -1.0)
    }

    pub fn then_skew_x(self, angle: f64) -> Self {
        self.multiply(Self::skew_x(angle))
    }

    pub fn then_skew_y(self, angle: f64) -> Self {
        self.multiply(Self::skew_y(angle))
    }

    pub fn determinant(self) -> f64 {
        self.a * self.d - self.c * self.b
    }

    pub fn has_finite_components(self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .into_iter()
            .all(f64::is_finite)
    }

    pub fn is_invertible(self) -> bool {
        let determinant = self.determinant();
        self.has_finite_components() && determinant.is_finite() && determinant != 0.0
    }

    pub fn inverse(self) -> Self {
        let determinant = self.determinant();
        if !self.is_invertible() {
            let nan = f64::NAN;
            return Self {
                a: nan,
                b: nan,
                c: nan,
                d: nan,
                e: nan,
                f: nan,
            };
        }
        Self {
            a: self.d / determinant,
            b: -self.b / determinant,
            c: -self.c / determinant,
            d: self.a / determinant,
            e: (self.c * self.f - self.d * self.e) / determinant,
            f: (self.b * self.e - self.a * self.f) / determinant,
        }
    }

    pub fn serialize_transform_matrix(self) -> String {
        format!(
            "matrix({} {} {} {} {} {})",
            serialize_number(self.a),
            serialize_number(self.b),
            serialize_number(self.c),
            serialize_number(self.d),
            serialize_number(self.e),
            serialize_number(self.f)
        )
    }
}

pub fn serialize_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}
