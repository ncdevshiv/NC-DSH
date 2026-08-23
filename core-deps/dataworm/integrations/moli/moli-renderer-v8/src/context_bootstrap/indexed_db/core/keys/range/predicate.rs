use super::*;

pub(in crate::context_bootstrap::indexed_db) fn key_in_range(
    key: &Key,
    range: &IdbKeyRangeQuery,
) -> bool {
    if let Some(lower) = &range.lower {
        match compare_idb_keys(key, lower) {
            -1 => return false,
            0 if range.lower_open => return false,
            _ => {}
        }
    }
    if let Some(upper) = &range.upper {
        match compare_idb_keys(key, upper) {
            1 => return false,
            0 if range.upper_open => return false,
            _ => {}
        }
    }
    true
}
