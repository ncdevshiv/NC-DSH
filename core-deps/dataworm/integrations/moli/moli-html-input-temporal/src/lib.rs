//! WHATWG HTML temporal input value microsyntax helpers.
//!
//! The parser shape follows Servo's split between strict HTML microsyntax
//! validation and `time`-backed calendar arithmetic. General-purpose datetime
//! crates do not model `<input>` value canonicalization closely enough for this
//! surface.

mod calendar;
mod constants;
mod date;
mod datetime;
mod month;
mod time;
mod week;

pub use constants::{
    MAX_DATE_INPUT_MILLISECONDS, MAX_INPUT_YEAR, MIN_DATE_INPUT_MILLISECONDS, MIN_INPUT_YEAR,
    MS_PER_DAY, MS_PER_HOUR, MS_PER_MINUTE, MS_PER_SECOND, MS_PER_WEEK, WEEK_INPUT_STEP_BASE,
};
pub use date::{
    date_input_milliseconds, date_input_value_from_milliseconds, is_valid_date_input_value,
};
pub use datetime::{
    datetime_local_input_milliseconds, datetime_local_input_value_from_milliseconds,
    is_valid_datetime_local_input_value,
};
pub use month::{
    is_valid_month_input_value, month_input_milliseconds, month_input_number,
    month_input_value_from_milliseconds, month_input_value_from_number,
};
pub use time::{
    is_valid_time_input_value, time_input_milliseconds, time_input_value_from_milliseconds,
};
pub use week::{
    is_valid_week_input_value, week_input_milliseconds, week_input_value_from_milliseconds,
};

#[cfg(test)]
mod tests;
