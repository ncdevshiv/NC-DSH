use cssparser::{Parser, ParserInput, Token};

use crate::unquote_css_string;

pub use style::moli_font_face::{CssFontFace, normalize_font_face_src, parse_font_faces};

pub fn font_load_query_contains_css_wide_keyword(query: &str) -> bool {
    let mut input = ParserInput::new(query);
    let mut input = Parser::new(&mut input);
    while let Ok(token) = input.next() {
        match token {
            Token::Ident(value) | Token::QuotedString(value)
                if is_css_wide_keyword(value.as_ref()) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

pub fn font_load_query_family(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut quote = None;
    let mut last_ws = None;
    for (index, ch) in trimmed.char_indices() {
        match ch {
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            _ if quote.is_none() && ch.is_whitespace() => last_ws = Some(index),
            _ => {}
        }
    }
    let family = last_ws
        .map(|index| trimmed[index..].trim())
        .unwrap_or(trimmed);
    Some(unquote_css_string(family))
}

fn is_css_wide_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "inherit" | "initial" | "unset" | "revert" | "revert-layer" | "revert-rule"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        font_load_query_contains_css_wide_keyword, font_load_query_family, normalize_font_face_src,
        parse_font_faces,
    };

    #[test]
    fn font_face_parser_uses_cssparser_rule_boundaries() {
        let entries = parse_font_faces(
            r#"
            .ignored { content: "@font-face { font-family: Bad; src: url(bad.woff2); }"; }
            @font-face {
                font-family: "A; B";
                src: url("data:font/woff2;base64;a;b");
            }
            @FONT-FACE {
                font-family: CaseFace;
                src: local("Case Face");
            }
            "#,
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].family, "A; B");
        assert_eq!(entries[0].source, r#"url("data:font/woff2;base64;a;b")"#);
        assert_eq!(entries[1].family, "CaseFace");
        assert_eq!(entries[1].source, r#"local("Case Face")"#);
    }

    #[test]
    fn font_face_parser_filters_invalid_and_incomplete_faces() {
        let entries = parse_font_faces(
            r#"
            @font-face { font-family: serif; src: url(generic.woff2); }
            @font-face { font-family: MissingSource; }
            @font-face { src: url(missing-family.woff2); }
            @font-face { font-family: Valid; src: url(valid.woff2); }
            "#,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].family, "Valid");
        assert_eq!(entries[0].source, r#"url("valid.woff2")"#);
    }

    #[test]
    fn font_face_src_normalizer_quotes_unquoted_urls() {
        assert_eq!(
            normalize_font_face_src("local(STIXGeneral), url(/stixfonts/STIXGeneral.otf)")
                .as_deref(),
            Some(r#"local(STIXGeneral), url("/stixfonts/STIXGeneral.otf")"#)
        );
        assert_eq!(
            normalize_font_face_src("url(http://foo/bar/font.ttf)").as_deref(),
            Some(r#"url("http://foo/bar/font.ttf")"#)
        );
    }

    #[test]
    fn font_load_query_uses_css_tokens_for_family_and_keywords() {
        assert!(font_load_query_contains_css_wide_keyword(
            r#"italic 16px inherit"#
        ));
        assert!(!font_load_query_contains_css_wide_keyword(
            r#"16px "inheritance""#
        ));
        assert_eq!(
            font_load_query_family(r#"italic small-caps bold 16px/2 "A B", serif"#).as_deref(),
            Some("serif")
        );
        assert_eq!(
            font_load_query_family(r#""Standalone Family""#).as_deref(),
            Some("Standalone Family")
        );
    }
}
