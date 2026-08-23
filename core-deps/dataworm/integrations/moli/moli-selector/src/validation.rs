use crate::selector::SelectorError;

/// DOM API selectors need a small amount of browser-specific validation on top
/// of parser errors. In particular, browsers reject malformed inputs that
/// cssparser-style recovery may otherwise accept.
pub(crate) fn pre_validate_selector(selector: &str) -> Result<(), SelectorError> {
    let mut bracket_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut chars = selector.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                chars.next();
            }
            '"' | '\'' => {
                let quote = ch;
                loop {
                    match chars.next() {
                        None | Some('\n') => break,
                        Some('\\') => {
                            chars.next();
                        }
                        Some(c) if c == quote => break,
                        Some(_) => {}
                    }
                }
            }
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return Err(SelectorError::syntax("unmatched ']' in selector"));
                }
            }
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return Err(SelectorError::syntax("unmatched ')' in selector"));
                }
            }
            '#' if bracket_depth == 0 && paren_depth == 0 => {
                if chars.peek().is_none_or(|next| {
                    matches!(next, ' ' | '\t' | '\n' | '\r' | '>' | '+' | '~' | ',' | ':')
                }) {
                    return Err(SelectorError::syntax("ID selectors require a name"));
                }
            }
            '.' if bracket_depth == 0 && paren_depth == 0 => {
                let mut lookahead = chars.clone();
                let class_starts_with_digit = lookahead.next().is_some_and(|next| {
                    next.is_ascii_digit()
                        || (next == '-'
                            && lookahead
                                .next()
                                .is_some_and(|after_dash| after_dash.is_ascii_digit()))
                });
                if class_starts_with_digit {
                    return Err(SelectorError::syntax(
                        "class selectors may not start with a digit",
                    ));
                }
            }
            _ => {}
        }
    }

    if bracket_depth != 0 {
        return Err(SelectorError::syntax("unclosed '[' in selector"));
    }
    if paren_depth != 0 {
        return Err(SelectorError::syntax("unclosed '(' in selector"));
    }

    Ok(())
}
