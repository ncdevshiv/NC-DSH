pub const MS_PER_SECOND: f64 = 1000.0;
pub const MS_PER_MINUTE: f64 = 60.0 * MS_PER_SECOND;
pub const MS_PER_HOUR: f64 = 60.0 * MS_PER_MINUTE;
pub const MS_PER_DAY: f64 = 24.0 * MS_PER_HOUR;
pub const MS_PER_WEEK: f64 = 7.0 * MS_PER_DAY;
pub const WEEK_INPUT_STEP_BASE: f64 = -259_200_000.0;
pub const MIN_INPUT_YEAR: i32 = 1;
pub const MAX_INPUT_YEAR: i32 = 275_760;
pub const MIN_DATE_INPUT_MILLISECONDS: f64 = -62_135_596_800_000.0;
pub const MAX_DATE_INPUT_MILLISECONDS: f64 = 8_640_000_000_000_000.0;

pub(crate) const MAX_MONTH_IN_MAX_YEAR: u8 = 9;
pub(crate) const MAX_DAY_IN_MAX_MONTH: u8 = 13;
pub(crate) const MAX_WEEK_IN_MAX_YEAR: u8 = 37;
