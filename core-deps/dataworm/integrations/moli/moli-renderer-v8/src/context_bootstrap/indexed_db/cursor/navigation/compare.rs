use super::*;

pub(super) fn cursor_direction_cmp(
    direction: CursorDirection,
    candidate: &Key,
    target: &Key,
) -> std::cmp::Ordering {
    moli_indexeddb::compare_cursor_direction(direction, candidate, target)
}

pub(super) fn cursor_tuple_cmp(
    direction: CursorDirection,
    candidate_key: &Key,
    candidate_primary_key: &Key,
    target_key: &Key,
    target_primary_key: &Key,
) -> std::cmp::Ordering {
    moli_indexeddb::compare_cursor_tuple_direction(
        direction,
        candidate_key,
        candidate_primary_key,
        target_key,
        target_primary_key,
    )
}
