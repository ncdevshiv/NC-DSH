use crate::length::parse_number_list;

pub(crate) fn number_len(value: &str) -> Option<usize> {
    moli_css_parse::number_len(value)
}

pub(crate) fn svg_number_list(raw: &str) -> Option<Vec<f64>> {
    parse_number_list(raw)
}
