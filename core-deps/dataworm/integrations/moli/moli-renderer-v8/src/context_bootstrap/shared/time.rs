pub(crate) fn unix_epoch_millis() -> f64 {
    moli_time::unix_epoch_millis()
}

pub(crate) fn dom_time_since_origin_millis(time_origin: f64) -> f64 {
    moli_time::dom_time_since_origin_millis(time_origin)
}
