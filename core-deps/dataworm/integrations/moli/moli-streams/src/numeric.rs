//! Numeric validation shared by Web Streams strategies and queues.

/// Why a queuing strategy high water mark is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighWaterMarkError {
    NaN,
    Negative,
}

/// Validates a Web Streams high water mark after Web IDL has converted it to
/// an unrestricted double.
///
/// Positive infinity and negative zero are valid high water marks. Property
/// access and JavaScript number conversion remain the runtime adapter's
/// responsibility.
pub fn validate_high_water_mark(value: f64) -> Result<f64, HighWaterMarkError> {
    if value.is_nan() {
        return Err(HighWaterMarkError::NaN);
    }
    if value < 0.0 {
        return Err(HighWaterMarkError::Negative);
    }
    Ok(value)
}

/// Why a queuing strategy size result is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueSizeError {
    NonFinite,
    Negative,
}

/// Validates the numeric result of a queuing strategy size algorithm.
///
/// This is intentionally narrower than Web IDL conversion: the runtime
/// adapter first invokes author JavaScript and converts the result to a
/// number, then passes that number across this boundary.
pub fn validate_queue_size(value: f64) -> Result<f64, QueueSizeError> {
    if !value.is_finite() {
        return Err(QueueSizeError::NonFinite);
    }
    if value < 0.0 {
        return Err(QueueSizeError::Negative);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_water_mark_accepts_the_streams_unrestricted_double_domain() {
        assert_eq!(validate_high_water_mark(0.0), Ok(0.0));
        assert_eq!(validate_high_water_mark(-0.0), Ok(-0.0));
        assert_eq!(validate_high_water_mark(1.5), Ok(1.5));
        assert_eq!(validate_high_water_mark(f64::INFINITY), Ok(f64::INFINITY));
        assert_eq!(
            validate_high_water_mark(f64::NAN),
            Err(HighWaterMarkError::NaN)
        );
        assert_eq!(
            validate_high_water_mark(-f64::INFINITY),
            Err(HighWaterMarkError::Negative)
        );
        assert_eq!(
            validate_high_water_mark(-1.0),
            Err(HighWaterMarkError::Negative)
        );
    }

    #[test]
    fn queue_size_requires_a_finite_non_negative_number() {
        assert_eq!(validate_queue_size(0.0), Ok(0.0));
        assert_eq!(validate_queue_size(-0.0), Ok(-0.0));
        assert_eq!(validate_queue_size(1.5), Ok(1.5));
        assert_eq!(
            validate_queue_size(f64::INFINITY),
            Err(QueueSizeError::NonFinite)
        );
        assert_eq!(
            validate_queue_size(f64::NEG_INFINITY),
            Err(QueueSizeError::NonFinite)
        );
        assert_eq!(
            validate_queue_size(f64::NAN),
            Err(QueueSizeError::NonFinite)
        );
        assert_eq!(validate_queue_size(-1.0), Err(QueueSizeError::Negative));
    }
}
