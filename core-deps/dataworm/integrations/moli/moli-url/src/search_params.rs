pub type SearchParamPair = (String, String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchParamsIteratorKind {
    Keys,
    Values,
    Entries,
}

impl SearchParamsIteratorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keys => "keys",
            Self::Values => "values",
            Self::Entries => "entries",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "keys" => Some(Self::Keys),
            "values" => Some(Self::Values),
            "entries" => Some(Self::Entries),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchParamsIteratorValue {
    String(String),
    Pair(SearchParamPair),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchParamsIteratorStep {
    pub done: bool,
    pub value: Option<SearchParamsIteratorValue>,
    pub next_index: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchParams {
    pairs: Vec<SearchParamPair>,
}

impl SearchParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pairs(pairs: Vec<SearchParamPair>) -> Self {
        Self { pairs }
    }

    pub fn parse(input: &str) -> Self {
        Self::from_pairs(parse_search_params(input))
    }

    pub fn from_url(url: &url::Url) -> Self {
        Self::from_pairs(url_query_pairs(url))
    }

    pub fn into_pairs(self) -> Vec<SearchParamPair> {
        self.pairs
    }

    pub fn as_pairs(&self) -> &[SearchParamPair] {
        &self.pairs
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value.as_str()))
    }

    pub fn get_all(&self, name: &str) -> Vec<String> {
        self.pairs
            .iter()
            .filter_map(|(key, value)| (key == name).then_some(value.clone()))
            .collect()
    }

    pub fn append(&mut self, name: String, value: String) {
        self.pairs.push((name, value));
    }

    pub fn set(&mut self, name: String, value: String) {
        let mut did_replace = false;
        let mut next = Vec::with_capacity(self.pairs.len().max(1));
        for (key, pair_value) in self.pairs.drain(..) {
            if key == name {
                if !did_replace {
                    next.push((name.clone(), value.clone()));
                    did_replace = true;
                }
                continue;
            }
            next.push((key, pair_value));
        }
        if !did_replace {
            next.push((name, value));
        }
        self.pairs = next;
    }

    pub fn delete(&mut self, name: &str, value: Option<&str>) {
        self.pairs.retain(|(key, pair_value)| {
            if key != name {
                return true;
            }
            value.is_some_and(|expected| pair_value != expected)
        });
    }

    pub fn has(&self, name: &str, value: Option<&str>) -> bool {
        self.pairs.iter().any(|(key, pair_value)| {
            key == name && value.is_none_or(|expected| pair_value == expected)
        })
    }

    pub fn sort(&mut self) {
        self.pairs
            .sort_by(|left, right| compare_utf16_code_units(&left.0, &right.0));
    }

    pub fn serialize(&self) -> Option<String> {
        serialize_search_params_pairs(&self.pairs)
    }
}

pub fn parse_search_params(input: &str) -> Vec<SearchParamPair> {
    let raw = input.strip_prefix('?').unwrap_or(input);
    if raw.is_empty() {
        return Vec::new();
    }
    url::form_urlencoded::parse(raw.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

pub fn url_query_pairs(url: &url::Url) -> Vec<SearchParamPair> {
    url.query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

pub fn serialize_search_params_pairs(pairs: &[SearchParamPair]) -> Option<String> {
    if pairs.is_empty() {
        return None;
    }
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    Some(serializer.finish())
}

pub fn compare_utf16_code_units(left: &str, right: &str) -> std::cmp::Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

pub fn search_params_iterator_step(
    pairs: &[SearchParamPair],
    index: usize,
    kind: SearchParamsIteratorKind,
) -> SearchParamsIteratorStep {
    let Some((key, value)) = pairs.get(index) else {
        return SearchParamsIteratorStep {
            done: true,
            value: None,
            next_index: index,
        };
    };
    let value = match kind {
        SearchParamsIteratorKind::Keys => SearchParamsIteratorValue::String(key.clone()),
        SearchParamsIteratorKind::Values => SearchParamsIteratorValue::String(value.clone()),
        SearchParamsIteratorKind::Entries => {
            SearchParamsIteratorValue::Pair((key.clone(), value.clone()))
        }
    };
    SearchParamsIteratorStep {
        done: false,
        value: Some(value),
        next_index: index.saturating_add(1),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SearchParamPair, SearchParams, SearchParamsIteratorKind, SearchParamsIteratorStep,
        SearchParamsIteratorValue, parse_search_params, search_params_iterator_step,
        serialize_search_params_pairs,
    };

    #[test]
    fn parses_and_serializes_form_urlencoded_pairs() {
        let pairs = parse_search_params("?a=1&space=hello+world&bad=%zz");
        assert_eq!(
            pairs,
            vec![
                ("a".to_owned(), "1".to_owned()),
                ("space".to_owned(), "hello world".to_owned()),
                ("bad".to_owned(), "%zz".to_owned())
            ]
        );
        assert_eq!(
            serialize_search_params_pairs(&pairs).as_deref(),
            Some("a=1&space=hello+world&bad=%25zz")
        );
    }

    #[test]
    fn mutates_pairs_with_url_search_params_semantics() {
        let mut params = SearchParams::parse("a=1&a=2&b=3");
        assert_eq!(params.get("a"), Some("1"));
        assert_eq!(params.get_all("a"), vec!["1".to_owned(), "2".to_owned()]);
        assert!(params.has("a", Some("2")));

        params.set("a".to_owned(), "updated".to_owned());
        params.append("a".to_owned(), "tail".to_owned());
        params.delete("b", None);

        assert_eq!(
            params.as_pairs(),
            &[
                ("a".to_owned(), "updated".to_owned()),
                ("a".to_owned(), "tail".to_owned())
            ]
        );
    }

    #[test]
    fn sorts_by_utf16_code_units() {
        let mut params = SearchParams::from_pairs(vec![
            ("\u{1f600}".to_owned(), "emoji".to_owned()),
            ("\u{fffd}".to_owned(), "replacement".to_owned()),
            ("a".to_owned(), "letter".to_owned()),
        ]);
        params.sort();
        assert_eq!(
            params
                .as_pairs()
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "\u{1f600}", "\u{fffd}"]
        );
    }

    #[test]
    fn iterator_step_projects_live_pair_by_kind() {
        let pairs: Vec<SearchParamPair> = vec![
            ("a".to_owned(), "1".to_owned()),
            ("b".to_owned(), "2".to_owned()),
        ];

        assert_eq!(
            search_params_iterator_step(&pairs, 0, SearchParamsIteratorKind::Keys),
            SearchParamsIteratorStep {
                done: false,
                value: Some(SearchParamsIteratorValue::String("a".to_owned())),
                next_index: 1,
            }
        );
        assert_eq!(
            search_params_iterator_step(&pairs, 1, SearchParamsIteratorKind::Values),
            SearchParamsIteratorStep {
                done: false,
                value: Some(SearchParamsIteratorValue::String("2".to_owned())),
                next_index: 2,
            }
        );
        assert_eq!(
            search_params_iterator_step(&pairs, 0, SearchParamsIteratorKind::Entries),
            SearchParamsIteratorStep {
                done: false,
                value: Some(SearchParamsIteratorValue::Pair((
                    "a".to_owned(),
                    "1".to_owned()
                ))),
                next_index: 1,
            }
        );
        assert_eq!(
            search_params_iterator_step(&pairs, 2, SearchParamsIteratorKind::Entries),
            SearchParamsIteratorStep {
                done: true,
                value: None,
                next_index: 2,
            }
        );
    }

    #[test]
    fn iterator_kind_labels_are_stable() {
        for (label, kind) in [
            ("keys", SearchParamsIteratorKind::Keys),
            ("values", SearchParamsIteratorKind::Values),
            ("entries", SearchParamsIteratorKind::Entries),
        ] {
            assert_eq!(SearchParamsIteratorKind::parse(label), Some(kind));
            assert_eq!(kind.as_str(), label);
        }
        assert_eq!(SearchParamsIteratorKind::parse("items"), None);
    }
}
