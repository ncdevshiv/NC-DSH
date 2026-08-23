//! A safe arithmetic expression evaluator for the `math` tool.
//!
//! A small recursive-descent parser supporting `+ - * /`, parentheses, and
//! decimal numbers. No `eval()`, no side effects.

/// Evaluates an arithmetic expression, e.g. `"6 * 7"` → `42.0`.
pub fn evaluate_expression(input: &str) -> Result<f64, String> {
    let mut parser = ExprParser {
        chars: input.chars().filter(|c| !c.is_whitespace()).collect(),
        pos: 0,
    };
    let value = parser.parse_expr()?;
    if parser.pos != parser.chars.len() {
        return Err(format!("unexpected trailing characters in `{input}`"));
    }
    Ok(value)
}

struct ExprParser {
    chars: Vec<char>,
    pos: usize,
}

impl ExprParser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn parse_expr(&mut self) -> Result<f64, String> {
        let mut value = self.parse_term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.next();
                    value += self.parse_term()?;
                }
                Some('-') => {
                    self.next();
                    value -= self.parse_term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_term(&mut self) -> Result<f64, String> {
        let mut value = self.parse_factor()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.next();
                    value *= self.parse_factor()?;
                }
                Some('/') => {
                    self.next();
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_factor(&mut self) -> Result<f64, String> {
        if self.peek() == Some('(') {
            self.next();
            let value = self.parse_expr()?;
            if self.next() != Some(')') {
                return Err("missing closing parenthesis".to_string());
            }
            return Ok(value);
        }
        // Optional leading minus.
        let mut digits = String::new();
        if self.peek() == Some('-') {
            digits.push('-');
            self.next();
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' {
                digits.push(c);
                self.next();
            } else {
                break;
            }
        }
        if digits.is_empty() || digits == "-" {
            return Err(format!("expected a number at position {}", self.pos));
        }
        digits
            .parse::<f64>()
            .map_err(|e| format!("invalid number `{digits}`: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        assert_eq!(evaluate_expression("6 * 7").unwrap(), 42.0);
        assert_eq!(evaluate_expression("2 + 3 * 4").unwrap(), 14.0);
        assert_eq!(evaluate_expression("(2 + 3) * 4").unwrap(), 20.0);
        assert_eq!(evaluate_expression("10 / 4").unwrap(), 2.5);
        assert_eq!(evaluate_expression("-5 + 10").unwrap(), 5.0);
    }

    #[test]
    fn errors() {
        assert!(evaluate_expression("6 / 0").is_err());
        assert!(evaluate_expression("(1 + 2").is_err());
        assert!(evaluate_expression("1 +").is_err());
        assert!(evaluate_expression("1 + 2x").is_err());
        assert!(evaluate_expression("").is_err());
    }
}
