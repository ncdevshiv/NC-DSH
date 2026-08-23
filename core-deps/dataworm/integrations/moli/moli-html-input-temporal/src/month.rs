use crate::{
    calendar::{date_milliseconds_from_parts, valid_month_parts, valid_year},
    date::date_input_value_from_milliseconds,
};

pub fn is_valid_month_input_value(value: &str) -> bool {
    value.is_empty() || parse_month_input_value(value).is_some()
}

pub fn month_input_number(value: &str) -> Option<f64> {
    let (year, month) = parse_month_input_value(value)?;
    Some(f64::from((year - 1970) * 12 + (i32::from(month) - 1)))
}

pub fn month_input_value_from_number(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let month_offset = value.round() as i32;
    let year = 1970 + month_offset.div_euclid(12);
    valid_year(year)?;
    let month = month_offset.rem_euclid(12) + 1;
    valid_month_parts(year, month as u8)?;
    Some(format!("{year:04}-{month:02}"))
}

pub fn month_input_milliseconds(value: &str) -> Option<f64> {
    let (year, month) = parse_month_input_value(value)?;
    date_milliseconds_from_parts(year, month, 1)
}

pub fn month_input_value_from_milliseconds(value: f64) -> Option<String> {
    date_input_value_from_milliseconds(value).map(|value| value[..value.len() - 3].to_owned())
}

fn parse_month_input_value(value: &str) -> Option<(i32, u8)> {
    let bytes = value.as_bytes();
    if bytes.len() < 7 {
        return None;
    }
    let year_end = bytes.len() - 3;
    if year_end < 4 || bytes[year_end] != b'-' {
        return None;
    }
    if !bytes[..year_end].iter().all(u8::is_ascii_digit)
        || !bytes[year_end + 1..year_end + 3]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    let year = value[..year_end].parse::<i32>().ok()?;
    valid_year(year)?;
    let month = value[year_end + 1..year_end + 3].parse::<u8>().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    valid_month_parts(year, month)?;
    Some((year, month))
}
