use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeaderFieldTokenMode {
    #[default]
    Normal,
    Relaxed,
}

#[derive(Clone, Debug)]
pub struct HeaderFieldTokenizer<'a> {
    input: &'a str,
    byte_index: usize,
}

impl<'a> HeaderFieldTokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut tokenizer = Self {
            input,
            byte_index: 0,
        };
        tokenizer.skip_optional_whitespace();
        tokenizer
    }

    pub fn byte_index(&self) -> usize {
        self.byte_index
    }

    pub fn is_consumed(&self) -> bool {
        self.byte_index >= self.input.len()
    }

    /// Consumes one separator and any following optional SP/HTAB whitespace.
    pub fn consume(&mut self, separator: char) -> bool {
        if !self.input[self.byte_index..].starts_with(separator) {
            return false;
        }
        self.byte_index += separator.len_utf8();
        self.skip_optional_whitespace();
        true
    }

    /// Consumes a non-empty ASCII token and following optional whitespace.
    pub fn consume_token(&mut self, mode: HeaderFieldTokenMode) -> Option<&'a str> {
        let start = self.byte_index;
        while let Some(character) = self.current_character() {
            if !token_character_is_valid(mode, character) {
                break;
            }
            self.byte_index += character.len_utf8();
        }
        if start == self.byte_index {
            return None;
        }
        let token = &self.input[start..self.byte_index];
        self.skip_optional_whitespace();
        Some(token)
    }

    /// Consumes a quoted-string, removes quoting backslashes, and skips
    /// following optional whitespace.
    pub fn consume_quoted_string(&mut self) -> Option<String> {
        if self.current_character()? != '"' {
            return None;
        }
        self.byte_index += 1;

        let mut output = String::new();
        while let Some(character) = self.current_character() {
            self.byte_index += character.len_utf8();
            match character {
                '"' => {
                    self.skip_optional_whitespace();
                    return Some(output);
                }
                '\\' => {
                    let escaped = self.current_character()?;
                    self.byte_index += escaped.len_utf8();
                    output.push(escaped);
                }
                _ => output.push(character),
            }
        }
        None
    }

    pub fn consume_token_or_quoted_string(
        &mut self,
        mode: HeaderFieldTokenMode,
    ) -> Option<Cow<'a, str>> {
        if self.current_character()? == '"' {
            self.consume_quoted_string().map(Cow::Owned)
        } else {
            self.consume_token(mode).map(Cow::Borrowed)
        }
    }

    /// Advances up to, but not including, the next matching character.
    ///
    /// Unlike token and separator consumption, this does not skip optional
    /// whitespace after advancing.
    pub fn consume_before_any_char_match(&mut self, characters: &[char]) {
        while let Some(character) = self.current_character() {
            if characters.contains(&character) {
                return;
            }
            self.byte_index += character.len_utf8();
        }
    }

    fn current_character(&self) -> Option<char> {
        self.input[self.byte_index..].chars().next()
    }

    fn skip_optional_whitespace(&mut self) {
        while self
            .input
            .as_bytes()
            .get(self.byte_index)
            .is_some_and(|byte| matches!(*byte, b' ' | b'\t'))
        {
            self.byte_index += 1;
        }
    }
}

fn token_character_is_valid(mode: HeaderFieldTokenMode, character: char) -> bool {
    if !character.is_ascii() || character < '\u{20}' {
        return false;
    }

    match character {
        ' ' | ';' | '"' => false,
        '(' | ')' | '<' | '>' | '@' | ',' | ':' | '\\' | '/' | '[' | ']' | '?' | '=' => {
            mode == HeaderFieldTokenMode::Relaxed
        }
        _ => true,
    }
}
