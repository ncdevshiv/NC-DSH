use super::numeric::is_valid_number_input_value;
use moli_html_input_temporal::{
    datetime_local_input_milliseconds, datetime_local_input_value_from_milliseconds,
    is_valid_date_input_value, is_valid_month_input_value, is_valid_time_input_value,
    is_valid_week_input_value,
};

pub fn canonical_input_type(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "button" => "button",
        "checkbox" => "checkbox",
        "color" => "color",
        "date" => "date",
        "datetime-local" => "datetime-local",
        "email" => "email",
        "file" => "file",
        "hidden" => "hidden",
        "image" => "image",
        "month" => "month",
        "number" => "number",
        "password" => "password",
        "radio" => "radio",
        "range" => "range",
        "reset" => "reset",
        "search" => "search",
        "submit" => "submit",
        "tel" => "tel",
        "text" => "text",
        "time" => "time",
        "url" => "url",
        "week" => "week",
        _ => "text",
    }
}

pub fn input_type_supports_value_as_number(input_type: &str) -> bool {
    matches!(
        canonical_input_type(input_type),
        "number" | "range" | "date" | "time" | "datetime-local" | "month" | "week"
    )
}

pub fn input_type_supports_pattern(input_type: &str) -> bool {
    matches!(
        canonical_input_type(input_type),
        "text" | "search" | "tel" | "url" | "email" | "password"
    )
}

pub fn input_type_supports_text_length_validation(input_type: &str) -> bool {
    matches!(
        canonical_input_type(input_type),
        "text" | "search" | "url" | "tel" | "email" | "password"
    )
}

pub fn input_type_suppresses_immutable_required(input_type: &str) -> bool {
    matches!(
        canonical_input_type(input_type),
        "text"
            | "search"
            | "url"
            | "tel"
            | "email"
            | "password"
            | "date"
            | "month"
            | "week"
            | "time"
            | "datetime-local"
            | "number"
    )
}

pub fn form_control_type_supports_intrinsic_validation(
    local_name: &str,
    input_type: Option<&str>,
    button_type: Option<&str>,
) -> bool {
    match local_name {
        "input" => !matches!(
            canonical_input_type(input_type.unwrap_or("text")),
            "hidden" | "button" | "reset"
        ),
        "select" | "textarea" => true,
        "button" => !matches!(
            button_type
                .unwrap_or("submit")
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "button" | "reset"
        ),
        _ => false,
    }
}

pub fn sanitize_input_value_for_type(input_type: &str, value: &str) -> String {
    match canonical_input_type(input_type) {
        // Text-family — strip newlines / CR but leave whitespace runs alone.
        "text" | "search" | "tel" | "password" => strip_input_value_line_breaks(value),
        // URL / Email — strip newlines AND trim leading/trailing ASCII whitespace.
        // (Email's multi-value parsing isn't relevant here since the dirty
        //  value path only ever sees a single value.)
        "url" | "email" => {
            let stripped = strip_input_value_line_breaks(value);
            stripped.trim_matches(is_ascii_whitespace_char).to_owned()
        }
        "number" if !is_valid_number_input_value(value) => String::new(),
        "date" if !is_valid_date_input_value(value) => String::new(),
        "time" if !is_valid_time_input_value(value) => String::new(),
        "datetime-local" => datetime_local_input_milliseconds(value)
            .and_then(datetime_local_input_value_from_milliseconds)
            .unwrap_or_default(),
        "month" if !is_valid_month_input_value(value) => String::new(),
        "week" if !is_valid_week_input_value(value) => String::new(),
        // Range — sanitization: parse as float, clamp to [min, max], else
        // default to "(min+max)/2". Without min/max context here we use the
        // attribute-less default range 0..=100 (midpoint 50). Callers with
        // attribute context can override.
        "range" => sanitize_range_value(value),
        // HTML-compatible color inputs accept CSS colors, discard alpha, and
        // expose an opaque lowercase sRGB simple color.
        "color" => sanitize_color_value(value),
        // File — IDL value is always the empty string when set
        // programmatically; the user-selected files are the only path to a
        // non-empty file list.
        "file" => String::new(),
        _ => value.to_owned(),
    }
}

fn strip_input_value_line_breaks(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, '\n' | '\r'))
        .collect()
}

fn is_ascii_whitespace_char(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

fn sanitize_range_value(value: &str) -> String {
    // Default min=0, max=100 per spec — without attribute access here we
    // can only honour that default; element-aware callers should override
    // by computing the suggested default themselves.
    match value.trim_matches(is_ascii_whitespace_char).parse::<f64>() {
        Ok(parsed) if parsed.is_finite() => {
            // Clamp to [0, 100]; preserve integer formatting when the result
            // is an integer to match the HTML serializer for valid floats.
            let clamped = parsed.clamp(0.0, 100.0);
            if clamped == clamped.trunc() {
                (clamped as i64).to_string()
            } else {
                clamped.to_string()
            }
        }
        _ => "50".to_owned(),
    }
}

fn sanitize_color_value(value: &str) -> String {
    moli_css_parse::parse_css_color_to_opaque_srgb_hex(value)
        .unwrap_or_else(|| "#000000".to_owned())
}

pub fn input_type_has_value_sanitization(input_type: &str) -> bool {
    matches!(
        canonical_input_type(input_type),
        "number" | "date" | "time" | "datetime-local" | "month" | "week"
    )
}

pub fn input_type_value_mismatch(input_type: &str, value: &str, multiple: bool) -> bool {
    if value.is_empty() {
        return false;
    }
    match canonical_input_type(input_type) {
        "email" => email_value_type_mismatch(value, multiple),
        "url" => url_value_type_mismatch(value),
        _ => false,
    }
}

pub fn email_value_type_mismatch(value: &str, multiple: bool) -> bool {
    if multiple {
        return value
            .split(',')
            .map(str::trim)
            .any(|address| address.is_empty() || !is_valid_email_address(address));
    }
    !is_valid_email_address(value.trim())
}

pub fn is_valid_email_address(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return false;
    }
    if !local.chars().all(is_email_atext_or_dot) {
        return false;
    }
    domain.split('.').all(is_valid_email_domain_label)
}

fn is_email_atext_or_dot(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '/'
                | '='
                | '?'
                | '^'
                | '_'
                | '`'
                | '{'
                | '|'
                | '}'
                | '~'
                | '.'
        )
}

fn is_valid_email_domain_label(label: &str) -> bool {
    if label.is_empty() || label.len() > 63 {
        return false;
    }
    let bytes = label.as_bytes();
    bytes[0].is_ascii_alphanumeric()
        && bytes[label.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

pub fn url_value_type_mismatch(value: &str) -> bool {
    url::Url::parse(value.trim()).is_err()
}

#[cfg(test)]
mod tests {
    use super::sanitize_input_value_for_type;

    const INITIAL: &str = "  foo\rbar  ";

    #[test]
    fn text_family_strips_cr_but_preserves_surrounding_whitespace() {
        for kw in ["text", "search", "tel", "password"] {
            assert_eq!(
                sanitize_input_value_for_type(kw, INITIAL),
                "  foobar  ",
                "{kw}: surrounding whitespace must survive"
            );
        }
    }

    #[test]
    fn url_email_strip_cr_then_trim_whitespace() {
        for kw in ["url", "email"] {
            assert_eq!(
                sanitize_input_value_for_type(kw, INITIAL),
                "foobar",
                "{kw}: must trim AND strip CR"
            );
            assert_eq!(sanitize_input_value_for_type(kw, ""), "");
            // Already-trimmed values pass through unchanged.
            assert_eq!(sanitize_input_value_for_type(kw, "foobar"), "foobar");
        }
    }

    #[test]
    fn range_defaults_to_midpoint_for_invalid_input() {
        // "  foo\rbar  " parses as NaN -> default midpoint of 0..=100 = "50".
        assert_eq!(sanitize_input_value_for_type("range", INITIAL), "50");
        // Empty -> default.
        assert_eq!(sanitize_input_value_for_type("range", ""), "50");
        // Valid float in-range round-trips and gets integer-serialised when
        // it lands exactly on an integer.
        assert_eq!(sanitize_input_value_for_type("range", "42"), "42");
        // Out-of-range -> clamped.
        assert_eq!(sanitize_input_value_for_type("range", "150"), "100");
        assert_eq!(sanitize_input_value_for_type("range", "-5"), "0");
    }

    #[test]
    fn color_parses_css_colors_and_defaults_to_black_for_invalid_input() {
        assert_eq!(sanitize_input_value_for_type("color", INITIAL), "#000000");
        assert_eq!(sanitize_input_value_for_type("color", ""), "#000000");
        assert_eq!(sanitize_input_value_for_type("color", "red"), "#ff0000");
        assert_eq!(sanitize_input_value_for_type("color", "#FFAA00"), "#ffaa00");
        assert_eq!(sanitize_input_value_for_type("color", "#abc"), "#aabbcc");
        assert_eq!(
            sanitize_input_value_for_type("color", "color(display-p3 .5 0 0)"),
            "#8c0000"
        );
        assert_eq!(
            sanitize_input_value_for_type("color", "not-a-color"),
            "#000000"
        );
    }

    #[test]
    fn file_always_yields_empty_string() {
        assert_eq!(sanitize_input_value_for_type("file", INITIAL), "");
        assert_eq!(sanitize_input_value_for_type("file", "anything"), "");
    }

    #[test]
    fn states_without_sanitization_pass_value_through_unchanged() {
        for kw in [
            "hidden", "checkbox", "radio", "submit", "reset", "button", "image",
        ] {
            assert_eq!(sanitize_input_value_for_type(kw, INITIAL), INITIAL);
        }
    }

    #[test]
    fn invalid_temporal_values_collapse_to_empty_string() {
        for kw in ["number", "date", "time", "datetime-local", "month", "week"] {
            assert_eq!(sanitize_input_value_for_type(kw, INITIAL), "");
        }
    }
}
