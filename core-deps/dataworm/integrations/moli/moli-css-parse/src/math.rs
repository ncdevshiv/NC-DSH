pub use style::moli_numeric::{
    ContainerQueryLengthContext, CssNumericContext, CssNumericKind, CssNumericValue, UnitlessAngle,
    UnitlessLength, css_number_value_is_supported, css_numeric_value_is_supported,
    css_time_value_is_supported, parse_angle_degrees, parse_number, parse_px_length,
    resolve_css_number, resolve_css_numeric, resolve_length_percentage, resolve_time_seconds,
    starts_with_supported_math_function,
};

pub fn balanced_function_len(raw: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in raw.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

pub fn number_len(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut index = 0;

    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }

    let integer_start = index;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
    }
    let integer_digits = index - integer_start;

    let mut fraction_digits = 0;
    if matches!(bytes.get(index), Some(b'.')) {
        index += 1;
        let fraction_start = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        fraction_digits = index - fraction_start;
    }

    if integer_digits == 0 && fraction_digits == 0 {
        return None;
    }

    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == exponent_start {
            return None;
        }
    }

    Some(index)
}

#[cfg(test)]
mod tests {
    use super::{
        ContainerQueryLengthContext, CssNumericContext, CssNumericKind, CssNumericValue,
        UnitlessAngle, UnitlessLength, balanced_function_len, css_number_value_is_supported,
        css_time_value_is_supported, number_len, parse_angle_degrees, parse_number,
        parse_px_length, resolve_css_number, resolve_css_numeric, resolve_length_percentage,
        resolve_time_seconds, starts_with_supported_math_function,
    };

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {actual} to be close to {expected}"
        );
    }

    #[test]
    fn number_scanner_handles_svg_and_css_numeric_tokens() {
        assert_eq!(number_len("-1.25e+3px"), Some(8));
        assert_eq!(number_len(".5"), Some(2));
        assert_eq!(number_len("e10"), None);
        assert_eq!(parse_number(" 12.5 "), Some(12.5));
        assert_eq!(parse_number("Infinity"), None);
    }

    #[test]
    fn css_math_lengths_and_angles_parse_absolute_values() {
        assert_close(parse_px_length("10px", UnitlessLength::Any).unwrap(), 10.0);
        assert_close(parse_px_length("10", UnitlessLength::Any).unwrap(), 10.0);
        assert_eq!(parse_px_length("10", UnitlessLength::ZeroOnly), None);
        assert_close(
            parse_px_length("calc(10px + 2px)", UnitlessLength::Any).unwrap(),
            12.0,
        );
        assert_close(
            parse_px_length("calc(2px * 3)", UnitlessLength::Any).unwrap(),
            6.0,
        );
        assert_close(
            parse_px_length("clamp(1px, max(2px, 3px), 4px)", UnitlessLength::Any).unwrap(),
            3.0,
        );
        assert_close(
            parse_angle_degrees("0.5turn", UnitlessAngle::Degrees).unwrap(),
            180.0,
        );
        assert_close(
            parse_angle_degrees("calc(90deg + 1rad)", UnitlessAngle::Degrees).unwrap(),
            90.0 + 180.0 / std::f64::consts::PI,
        );
    }

    #[test]
    fn balanced_function_helpers_find_supported_css_math() {
        assert!(starts_with_supported_math_function("calc(1px + 2px)"));
        assert!(starts_with_supported_math_function("round(up, 2.2px, 1px)"));
        assert_eq!(balanced_function_len("calc(min(1px, 2px)) tail"), Some(19));
        assert_eq!(balanced_function_len("calc(1px"), None);
    }

    #[test]
    fn css_math_uses_stylo_parser_for_extended_functions_and_unit_algebra() {
        assert_close(
            parse_px_length("abs(-2px)", UnitlessLength::ZeroOnly).unwrap(),
            2.0,
        );
        assert_close(
            parse_px_length("round(up, 2.2px, 1px)", UnitlessLength::ZeroOnly).unwrap(),
            3.0,
        );
        assert_eq!(resolve_css_number("calc(5px / 1px)", None), None);
        assert_eq!(
            parse_px_length("calc(5px / 1px)", UnitlessLength::ZeroOnly),
            None
        );
    }

    #[test]
    fn css_math_resolves_length_percentages_against_basis() {
        assert_close(
            resolve_length_percentage("25%", 200.0, UnitlessLength::ZeroOnly).unwrap(),
            50.0,
        );
        assert_close(
            resolve_length_percentage("calc(25% - 2px)", 200.0, UnitlessLength::ZeroOnly).unwrap(),
            48.0,
        );
        assert_close(
            resolve_length_percentage("max(10%, 3px)", 20.0, UnitlessLength::ZeroOnly).unwrap(),
            3.0,
        );
        assert_close(
            resolve_length_percentage(
                "max(10px + (2 * (10px + min(10%, 30px))), 5% + 80px)",
                100.0,
                UnitlessLength::ZeroOnly,
            )
            .unwrap(),
            85.0,
        );
    }

    #[test]
    fn css_math_resolves_percentages_without_accepting_lengths() {
        assert_close(
            resolve_css_numeric(
                "calc(min(50%, 60%))",
                CssNumericKind::Percentage,
                CssNumericContext::supports_probe(),
            )
            .unwrap()
            .percentage()
            .unwrap(),
            50.0,
        );
        assert!(
            resolve_css_numeric(
                "calc(50px - 50%)",
                CssNumericKind::Percentage,
                CssNumericContext::supports_probe(),
            )
            .is_none()
        );
    }

    #[test]
    fn css_math_resolves_sign_with_container_units_for_animation_values() {
        let container = ContainerQueryLengthContext::from_inline_size(100.0);
        assert!(css_time_value_is_supported(
            "calc(10s + (sign(2cqw - 10px) * 5s))"
        ));
        assert!(css_number_value_is_supported(
            "calc(10 + (sign(2cqw - 10px) * 5))"
        ));
        assert_close(
            resolve_time_seconds("calc(10s + (sign(2cqw - 10px) * 5s))", Some(container)).unwrap(),
            5.0,
        );
        assert_close(
            resolve_css_number("calc(10 + (sign(2cqw - 10px) * 5))", Some(container)).unwrap(),
            5.0,
        );
    }

    #[test]
    fn css_math_resolves_shared_numeric_context_units() {
        let context = CssNumericContext {
            container_lengths: Some(ContainerQueryLengthContext::from_inline_size(200.0)),
            font_size_px: Some(16.0),
            root_font_size_px: Some(30.0),
            line_height_px: Some(20.0),
            viewport_width_px: Some(1920.0),
            viewport_height_px: Some(1080.0),
            ..CssNumericContext::default()
        };
        assert_close(
            resolve_css_numeric(
                "10%",
                CssNumericKind::LengthPercentage {
                    basis: 520.0,
                    unitless: UnitlessLength::ZeroOnly,
                },
                context,
            )
            .unwrap()
            .px_length()
            .unwrap(),
            52.0,
        );
        assert_close(
            resolve_css_numeric(
                "2em",
                CssNumericKind::LengthPercentage {
                    basis: 520.0,
                    unitless: UnitlessLength::ZeroOnly,
                },
                context,
            )
            .unwrap()
            .px_length()
            .unwrap(),
            32.0,
        );
        assert!(
            resolve_css_numeric(
                "calc(10% + 2px)",
                CssNumericKind::LengthPercentage {
                    basis: 520.0,
                    unitless: UnitlessLength::ZeroOnly,
                },
                context,
            )
            .is_some(),
            "percentage plus px should resolve"
        );
        assert!(
            resolve_css_numeric(
                "calc(10px + 2em)",
                CssNumericKind::LengthPercentage {
                    basis: 520.0,
                    unitless: UnitlessLength::ZeroOnly,
                },
                context,
            )
            .is_some(),
            "px plus em should resolve"
        );
        assert_close(
            resolve_css_numeric(
                "calc(10% + 2em)",
                CssNumericKind::LengthPercentage {
                    basis: 520.0,
                    unitless: UnitlessLength::ZeroOnly,
                },
                context,
            )
            .unwrap()
            .px_length()
            .unwrap(),
            84.0,
        );
        assert_close(
            resolve_css_numeric(
                "calc(8lh + 7px)",
                CssNumericKind::LengthPercentage {
                    basis: 100.0,
                    unitless: UnitlessLength::ZeroOnly,
                },
                context,
            )
            .unwrap()
            .px_length()
            .unwrap(),
            167.0,
        );
        let resolve_width = |value| {
            resolve_css_numeric(
                value,
                CssNumericKind::LengthPercentage {
                    basis: 520.0,
                    unitless: UnitlessLength::ZeroOnly,
                },
                context,
            )
            .and_then(CssNumericValue::px_length)
        };
        assert_close(resolve_width("calc(5px * 10)").unwrap(), 50.0);
        assert_close(resolve_width("calc(20% * 0.5)").unwrap(), 52.0);
        assert_close(resolve_width("calc(4px * 4)").unwrap(), 16.0);
        assert_close(resolve_width("calc(400px / 4)").unwrap(), 100.0);
        assert_close(resolve_width("calc((20% + 1em) * 0.5)").unwrap(), 60.0);
        assert_close(resolve_width("calc(100px / 1 / 1)").unwrap(), 100.0);
        assert_eq!(resolve_width("calc(5px * 10lh / 1px)"), None);
        assert_eq!(resolve_width("calc(20% * 0.5em / 1px)"), None);
        assert_eq!(resolve_width("calc(400px / 4lh * 1px)"), None);
        assert_eq!(resolve_width("calc(20% / 0.5em * 1px)"), None);
        assert_eq!(resolve_width("calc(52px * 1px / 10%)"), None);
        assert_eq!(resolve_width("calc(100px * 1px / 1px / 1)"), None);
        assert_close(
            resolve_css_numeric(
                "calc(10 + sign(1em - 1000px))",
                CssNumericKind::Number,
                context,
            )
            .unwrap()
            .number()
            .unwrap(),
            9.0,
        );
        assert_close(
            resolve_css_numeric("calc(10 + sign(1 - 2))", CssNumericKind::Number, context)
                .unwrap()
                .number()
                .unwrap(),
            9.0,
        );
        assert_close(
            resolve_css_numeric(
                "calc(10 + sign(30deg - 40deg))",
                CssNumericKind::Number,
                context,
            )
            .unwrap()
            .number()
            .unwrap(),
            9.0,
        );
        assert_close(
            resolve_css_numeric(
                "calc(2 * sibling-index())",
                CssNumericKind::Number,
                CssNumericContext {
                    sibling_index: Some(3.0),
                    ..context
                },
            )
            .unwrap()
            .number()
            .unwrap(),
            6.0,
        );
    }
}
