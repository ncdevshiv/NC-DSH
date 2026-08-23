use crate::{MS_PER_DAY, MS_PER_HOUR, MS_PER_MINUTE, MS_PER_SECOND};

pub fn is_valid_time_input_value(value: &str) -> bool {
    value.is_empty() || time_input_milliseconds(value).is_some()
}

pub fn time_input_milliseconds(value: &str) -> Option<f64> {
    let bytes = value.as_bytes();
    if bytes.len() < 5 || bytes[2] != b':' {
        return None;
    }
    if !bytes[..2].iter().all(u8::is_ascii_digit) || !bytes[3..5].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let hour = value[0..2].parse::<u32>().ok()?;
    let minute = value[3..5].parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let mut total = (f64::from(hour) * MS_PER_HOUR) + (f64::from(minute) * MS_PER_MINUTE);
    if bytes.len() == 5 {
        return Some(total);
    }
    if bytes.len() < 8 || bytes[5] != b':' || !bytes[6..8].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let second = value[6..8].parse::<u32>().ok()?;
    if second > 59 {
        return None;
    }
    total += f64::from(second) * MS_PER_SECOND;
    if bytes.len() == 8 {
        return Some(total);
    }
    if bytes[8] != b'.' || bytes.len() == 9 || bytes.len() > 12 {
        return None;
    }
    if !bytes[9..].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let fraction = &value[9..];
    let millis = match fraction.len() {
        1 => fraction.parse::<u32>().ok()? * 100,
        2 => fraction.parse::<u32>().ok()? * 10,
        3 => fraction.parse::<u32>().ok()?,
        _ => return None,
    };
    Some(total + f64::from(millis))
}

pub fn time_input_value_from_milliseconds(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    // Blink DateComponents normalizes time numbers with positive modulo before
    // formatting, so callers do not need to duplicate type=time wrapping rules.
    let total_millis = value.rem_euclid(MS_PER_DAY).floor() as u32;
    let hour = total_millis / (MS_PER_HOUR as u32);
    let minute = (total_millis % (MS_PER_HOUR as u32)) / (MS_PER_MINUTE as u32);
    let second = (total_millis % (MS_PER_MINUTE as u32)) / (MS_PER_SECOND as u32);
    let millis = total_millis % (MS_PER_SECOND as u32);
    if millis > 0 {
        let mut fraction = format!("{millis:03}");
        while fraction.ends_with('0') {
            fraction.pop();
        }
        return Some(format!("{hour:02}:{minute:02}:{second:02}.{fraction}"));
    }
    if second > 0 {
        return Some(format!("{hour:02}:{minute:02}:{second:02}"));
    }
    Some(format!("{hour:02}:{minute:02}"))
}
