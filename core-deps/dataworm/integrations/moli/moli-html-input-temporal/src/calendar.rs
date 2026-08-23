use crate::{
    MAX_DATE_INPUT_MILLISECONDS, MAX_INPUT_YEAR, MIN_DATE_INPUT_MILLISECONDS, MIN_INPUT_YEAR,
    MS_PER_DAY,
    constants::{MAX_DAY_IN_MAX_MONTH, MAX_MONTH_IN_MAX_YEAR, MAX_WEEK_IN_MAX_YEAR},
};

pub(crate) fn date_milliseconds_from_parts(year: i32, month: u8, day: u8) -> Option<f64> {
    valid_calendar_date_parts(year, month, day)?;
    let month = time::Month::try_from(month).ok()?;
    let date = time::Date::from_calendar_date(year, month, day).ok()?;
    let epoch = unix_epoch_date()?;
    Some(((date - epoch).whole_days() as f64) * MS_PER_DAY)
}

pub(crate) fn unix_epoch_date() -> Option<time::Date> {
    time::Date::from_calendar_date(1970, time::Month::January, 1).ok()
}

pub(crate) fn date_from_epoch_day_offset(days: f64) -> Option<time::Date> {
    let milliseconds = days * MS_PER_DAY;
    if !days.is_finite()
        || !(MIN_DATE_INPUT_MILLISECONDS..=MAX_DATE_INPUT_MILLISECONDS).contains(&milliseconds)
    {
        return None;
    }
    let epoch_julian_day = i64::from(unix_epoch_date()?.to_julian_day());
    let julian_day = epoch_julian_day.checked_add(days as i64)?;
    let julian_day = i32::try_from(julian_day).ok()?;
    time::Date::from_julian_day(julian_day).ok()
}

pub(crate) fn valid_year(year: i32) -> Option<i32> {
    (MIN_INPUT_YEAR..=MAX_INPUT_YEAR)
        .contains(&year)
        .then_some(year)
}

pub(crate) fn valid_date_milliseconds(value: f64) -> bool {
    value.is_finite()
        && (MIN_DATE_INPUT_MILLISECONDS..=MAX_DATE_INPUT_MILLISECONDS).contains(&value)
}

pub(crate) fn rounded_date_milliseconds(value: f64) -> Option<f64> {
    value.is_finite().then(|| value.round())
}

pub(crate) fn valid_month_parts(year: i32, month: u8) -> Option<()> {
    if year < MAX_INPUT_YEAR {
        return Some(());
    }
    (month <= MAX_MONTH_IN_MAX_YEAR).then_some(())
}

pub(crate) fn valid_calendar_date_parts(year: i32, month: u8, day: u8) -> Option<()> {
    if year < MAX_INPUT_YEAR || month < MAX_MONTH_IN_MAX_YEAR {
        return Some(());
    }
    (month == MAX_MONTH_IN_MAX_YEAR && day <= MAX_DAY_IN_MAX_MONTH).then_some(())
}

pub(crate) fn valid_datetime_local_parts(date: time::Date, time_milliseconds: f64) -> Option<()> {
    valid_calendar_date_parts(date.year(), u8::from(date.month()), date.day())?;
    if date.year() < MAX_INPUT_YEAR
        || u8::from(date.month()) < MAX_MONTH_IN_MAX_YEAR
        || date.day() < MAX_DAY_IN_MAX_MONTH
    {
        return Some(());
    }
    (time_milliseconds == 0.0).then_some(())
}

pub(crate) fn valid_week_parts(year: i32, week: u8) -> Option<()> {
    if year < MAX_INPUT_YEAR {
        return Some(());
    }
    (week <= MAX_WEEK_IN_MAX_YEAR).then_some(())
}
