pub(in crate::native_bridge) fn webidl_long_from_number(value: f64) -> i32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    let integer = value.signum() * value.abs().floor();
    let modulo = integer.rem_euclid(4_294_967_296.0);
    let signed = if modulo >= 2_147_483_648.0 {
        modulo - 4_294_967_296.0
    } else {
        modulo
    };
    signed as i32
}

#[cfg(test)]
mod tests {
    use super::webidl_long_from_number;

    #[test]
    fn webidl_long_conversion_handles_special_and_wrapping_values() {
        assert_eq!(webidl_long_from_number(f64::NAN), 0);
        assert_eq!(webidl_long_from_number(f64::INFINITY), 0);
        assert_eq!(webidl_long_from_number(f64::NEG_INFINITY), 0);
        assert_eq!(webidl_long_from_number(2.75), 2);
        assert_eq!(webidl_long_from_number(-2.75), -2);
        assert_eq!(webidl_long_from_number(4_294_967_297.0), 1);
        assert_eq!(webidl_long_from_number(2_147_483_648.0), -2_147_483_648);
    }
}
