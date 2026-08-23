use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumString)]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
pub(super) enum InsertAdjacentPosition {
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
}

pub(super) fn parse_insert_adjacent_position(position: &str) -> Option<InsertAdjacentPosition> {
    InsertAdjacentPosition::from_str(position).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_adjacent_position_parse_matches_html_tokens() {
        assert_eq!(
            parse_insert_adjacent_position("beforebegin"),
            Some(InsertAdjacentPosition::BeforeBegin)
        );
        assert_eq!(
            parse_insert_adjacent_position("AfterBegin"),
            Some(InsertAdjacentPosition::AfterBegin)
        );
        assert_eq!(
            parse_insert_adjacent_position("BEFOREEND"),
            Some(InsertAdjacentPosition::BeforeEnd)
        );
        assert_eq!(
            parse_insert_adjacent_position("afterend"),
            Some(InsertAdjacentPosition::AfterEnd)
        );
        assert_eq!(parse_insert_adjacent_position("before begin"), None);
    }
}
