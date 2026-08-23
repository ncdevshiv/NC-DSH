use crate::{
    MS_PER_DAY,
    calendar::{
        date_from_epoch_day_offset, rounded_date_milliseconds, unix_epoch_date,
        valid_date_milliseconds, valid_datetime_local_parts, valid_year,
    },
    date::parse_date_input_date,
    time::{time_input_milliseconds, time_input_value_from_milliseconds},
};

pub fn is_valid_datetime_local_input_value(value: &str) -> bool {
    value.is_empty() || parse_datetime_local_input_value(value).is_some()
}

pub fn datetime_local_input_milliseconds(value: &str) -> Option<f64> {
    let (date, time) = parse_datetime_local_input_value(value)?;
    valid_datetime_local_parts(date, time)?;
    let epoch = unix_epoch_date()?;
    Some(((date - epoch).whole_days() as f64) * MS_PER_DAY + time)
}

pub fn datetime_local_input_value_from_milliseconds(value: f64) -> Option<String> {
    let value = rounded_date_milliseconds(value)?;
    if !valid_date_milliseconds(value) {
        return None;
    }
    let days = (value / MS_PER_DAY).floor();
    let day_millis = value - (days * MS_PER_DAY);
    let date = date_from_epoch_day_offset(days)?;
    valid_year(date.year())?;
    let time = time_input_value_from_milliseconds(day_millis)?;
    Some(format!(
        "{:04}-{:02}-{:02}T{}",
        date.year(),
        u8::from(date.month()),
        date.day(),
        time
    ))
}

fn parse_datetime_local_input_value(value: &str) -> Option<(time::Date, f64)> {
    let bytes = value.as_bytes();
    let separator = bytes
        .iter()
        .position(|byte| matches!(byte, b'T' | b' '))
        .filter(|separator| *separator >= 10)?;
    let date = parse_date_input_date(&value[..separator])?;
    let time = time_input_milliseconds(&value[separator + 1..])?;
    valid_datetime_local_parts(date, time)?;
    Some((date, time))
}
