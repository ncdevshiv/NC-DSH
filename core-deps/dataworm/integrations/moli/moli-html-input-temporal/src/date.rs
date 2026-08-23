use crate::{
    MS_PER_DAY,
    calendar::{
        date_from_epoch_day_offset, rounded_date_milliseconds, unix_epoch_date,
        valid_calendar_date_parts, valid_date_milliseconds, valid_year,
    },
};

pub fn is_valid_date_input_value(value: &str) -> bool {
    value.is_empty() || parse_date_input_date(value).is_some()
}

pub fn date_input_milliseconds(value: &str) -> Option<f64> {
    let date = parse_date_input_date(value)?;
    let epoch = unix_epoch_date()?;
    Some(((date - epoch).whole_days() as f64) * MS_PER_DAY)
}

pub fn date_input_value_from_milliseconds(value: f64) -> Option<String> {
    let value = rounded_date_milliseconds(value)?;
    if !valid_date_milliseconds(value) {
        return None;
    }
    let date = date_from_epoch_day_offset((value / MS_PER_DAY).floor())?;
    valid_year(date.year())?;
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    ))
}

pub(crate) fn parse_date_input_date(value: &str) -> Option<time::Date> {
    let bytes = value.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let year_end = bytes.len() - 6;
    if year_end < 4 || bytes[year_end] != b'-' || bytes[year_end + 3] != b'-' {
        return None;
    }
    if !bytes[..year_end].iter().all(u8::is_ascii_digit)
        || !bytes[year_end + 1..year_end + 3]
            .iter()
            .all(u8::is_ascii_digit)
        || !bytes[year_end + 4..year_end + 6]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    let year = value[..year_end].parse::<i32>().ok()?;
    valid_year(year)?;
    let month = value[year_end + 1..year_end + 3].parse::<u8>().ok()?;
    let day = value[year_end + 4..year_end + 6].parse::<u8>().ok()?;
    valid_calendar_date_parts(year, month, day)?;
    let month = time::Month::try_from(month).ok()?;
    time::Date::from_calendar_date(year, month, day).ok()
}
