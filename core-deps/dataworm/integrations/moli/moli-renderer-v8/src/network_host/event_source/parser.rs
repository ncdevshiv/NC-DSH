const DEFAULT_RECONNECT_DELAY_MS: u64 = 3_000;
const UTF8_BOM: &[u8] = &[0xef, 0xbb, 0xbf];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EventSourceMessage {
    pub(crate) event_name: String,
    pub(crate) event_id: String,
    pub(crate) data: String,
}

#[derive(Debug)]
pub(crate) struct EventSourceParser {
    line: Vec<u8>,
    data: Vec<u8>,
    event_name: String,
    pending_event_id: String,
    last_event_id: String,
    reconnect_delay_ms: u64,
    recognizing_bom: bool,
    recognizing_crlf: bool,
}

impl EventSourceParser {
    pub(crate) fn new(last_event_id: String, reconnect_delay_ms: u64) -> Self {
        Self {
            line: Vec::new(),
            data: Vec::new(),
            event_name: String::new(),
            pending_event_id: last_event_id.clone(),
            last_event_id,
            reconnect_delay_ms,
            recognizing_bom: true,
            recognizing_crlf: false,
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<EventSourceMessage> {
        let mut messages = Vec::new();
        for byte in bytes.iter().copied() {
            if self.recognizing_bom {
                self.line.push(byte);
                if UTF8_BOM.starts_with(&self.line) {
                    if self.line.len() == UTF8_BOM.len() {
                        self.line.clear();
                        self.recognizing_bom = false;
                    }
                    continue;
                }
                self.recognizing_bom = false;
                let prefix = std::mem::take(&mut self.line);
                for prefix_byte in prefix {
                    self.push_line_byte(prefix_byte, &mut messages);
                }
                continue;
            }
            self.push_line_byte(byte, &mut messages);
        }
        messages
    }

    pub(crate) fn last_event_id(&self) -> &str {
        &self.last_event_id
    }

    pub(crate) fn reconnect_delay_ms(&self) -> u64 {
        self.reconnect_delay_ms
    }

    fn push_line_byte(&mut self, byte: u8, messages: &mut Vec<EventSourceMessage>) {
        if self.recognizing_crlf {
            self.recognizing_crlf = false;
            if byte == b'\n' {
                return;
            }
        }
        if matches!(byte, b'\r' | b'\n') {
            self.parse_line(messages);
            self.line.clear();
            self.recognizing_crlf = byte == b'\r';
        } else {
            self.line.push(byte);
        }
    }

    fn parse_line(&mut self, messages: &mut Vec<EventSourceMessage>) {
        if self.line.is_empty() {
            self.last_event_id.clone_from(&self.pending_event_id);
            if !self.data.is_empty() {
                debug_assert_eq!(self.data.last(), Some(&b'\n'));
                self.data.pop();
                messages.push(EventSourceMessage {
                    event_name: if self.event_name.is_empty() {
                        "message".to_owned()
                    } else {
                        std::mem::take(&mut self.event_name)
                    },
                    event_id: self.last_event_id.clone(),
                    data: String::from_utf8_lossy(&self.data).into_owned(),
                });
                self.data.clear();
            }
            self.event_name.clear();
            return;
        }

        let field_name_end = self
            .line
            .iter()
            .position(|byte| *byte == b':')
            .unwrap_or(self.line.len());
        let mut field_value_start = field_name_end;
        if field_name_end < self.line.len() {
            field_value_start += 1;
            if self.line.get(field_value_start) == Some(&b' ') {
                field_value_start += 1;
            }
        }
        let field_name = String::from_utf8_lossy(&self.line[..field_name_end]);
        let field_value = &self.line[field_value_start..];
        match field_name.as_ref() {
            "event" => {
                self.event_name = String::from_utf8_lossy(field_value).into_owned();
            }
            "data" => {
                self.data.extend_from_slice(field_value);
                self.data.push(b'\n');
            }
            "id" if !field_value.contains(&0) => {
                self.pending_event_id = String::from_utf8_lossy(field_value).into_owned();
            }
            "retry" if field_value.is_empty() => {
                self.reconnect_delay_ms = DEFAULT_RECONNECT_DELAY_MS;
            }
            "retry" if field_value.iter().all(u8::is_ascii_digit) => {
                if let Ok(value) = std::str::from_utf8(field_value)
                    .unwrap_or_default()
                    .parse::<u64>()
                {
                    self.reconnect_delay_ms = value;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_RECONNECT_DELAY_MS, EventSourceMessage, EventSourceParser};

    fn message(event_name: &str, event_id: &str, data: &str) -> EventSourceMessage {
        EventSourceMessage {
            event_name: event_name.to_owned(),
            event_id: event_id.to_owned(),
            data: data.to_owned(),
        }
    }

    #[test]
    fn parses_messages_incrementally_across_crlf_and_utf8_boundaries() {
        let mut parser = EventSourceParser::new(String::new(), DEFAULT_RECONNECT_DELAY_MS);

        assert!(parser.push(b"\xef").is_empty());
        assert!(parser.push(b"\xbb\xbfid: 7\r").is_empty());
        assert!(parser.push(b"\ndata: hel").is_empty());
        assert!(parser.push(b"lo \xe4").is_empty());
        assert!(parser.push(b"\xb8\x96\xe7\x95\x8c\r\n").is_empty());
        assert_eq!(
            parser.push(b"\r\n"),
            vec![message("message", "7", "hello \u{4e16}\u{754c}")]
        );
        assert_eq!(parser.last_event_id(), "7");
    }

    #[test]
    fn parses_custom_events_multiline_data_and_retry() {
        let mut parser = EventSourceParser::new("old".to_owned(), DEFAULT_RECONNECT_DELAY_MS);
        let messages =
            parser.push(b"event: update\ndata: first\ndata: second\nretry: 25\nid: next\n\n");

        assert_eq!(messages, vec![message("update", "next", "first\nsecond")]);
        assert_eq!(parser.reconnect_delay_ms(), 25);
        assert_eq!(parser.last_event_id(), "next");

        assert!(parser.push(b"retry:\n\n").is_empty());
        assert_eq!(parser.reconnect_delay_ms(), DEFAULT_RECONNECT_DELAY_MS);
    }

    #[test]
    fn ignores_null_ids_invalid_retry_and_unterminated_events() {
        let mut parser = EventSourceParser::new("kept".to_owned(), DEFAULT_RECONNECT_DELAY_MS);

        assert!(
            parser
                .push(b"id: bad\0id\nretry: 1x\ndata: pending")
                .is_empty()
        );
        assert_eq!(parser.last_event_id(), "kept");
        assert_eq!(parser.reconnect_delay_ms(), DEFAULT_RECONNECT_DELAY_MS);

        assert_eq!(
            parser.push(b"\n\n"),
            vec![message("message", "kept", "pending")]
        );
        assert_eq!(
            parser.push(b"data: replacement \xff\n\n"),
            vec![message("message", "kept", "replacement \u{fffd}")]
        );
    }

    #[test]
    fn commits_id_on_blank_line_without_dispatching_data() {
        let mut parser = EventSourceParser::new(String::new(), DEFAULT_RECONNECT_DELAY_MS);

        assert!(parser.push(b"id: committed\n\n").is_empty());
        assert_eq!(parser.last_event_id(), "committed");
    }
}
