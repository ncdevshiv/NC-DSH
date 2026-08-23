use crate::{
    MS_PER_DAY,
    calendar::{
        date_from_epoch_day_offset, rounded_date_milliseconds, unix_epoch_date,
        valid_date_milliseconds, valid_week_parts, valid_year,
    },
};

pub fn is_valid_week_input_value(value: &str) -> bool {
    value.is_empty() || parse_week_input_value(value).is_some()
}

pub fn week_input_milliseconds(value: &str) -> Option<f64> {
    let (year, week) = parse_week_input_value(value)?;
    let date = time::Date::from_iso_week_date(year, week, time::Weekday::Monday).ok()?;
    let epoch = unix_epoch_date()?;
    Some(((date - epoch).whole_days() as f64) * MS_PER_DAY)
}

pub fn week_input_value_from_milliseconds(value: f64) -> Option<String> {
    let value = rounded_date_milliseconds(value)?;
    if !valid_date_milliseconds(value) {
        return None;
    }
    let date = date_from_epoch_day_offset((value / MS_PER_DAY).floor())?;
    let (year, week, _) = date.to_iso_week_date();
    valid_year(year)?;
    Some(format!("{year:04}-W{week:02}"))
}

fn parse_week_input_value(value: &str) -> Option<(i32, u8)> {
    let bytes = value.as_bytes();
    if bytes.len() < 8 {
        return None;
    }
    let year_end = bytes.len() - 4;
    if year_end < 4 || bytes[year_end] != b'-' || bytes[year_end + 1] != b'W' {
        return None;
    }
    if !bytes[..year_end].iter().all(u8::is_ascii_digit)
        || !bytes[year_end + 2..year_end + 4]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    let year = value[..year_end].parse::<i32>().ok()?;
    valid_year(year)?;
    let week = value[year_end + 2..year_end + 4].parse::<u8>().ok()?;
    valid_week_parts(year, week)?;
    time::Date::from_iso_week_date(year, week, time::Weekday::Monday).ok()?;
    Some((year, week))
}
