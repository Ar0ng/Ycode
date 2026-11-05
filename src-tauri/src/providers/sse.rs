//! Minimal incremental Server-Sent-Events parser (WHATWG subset: `event:`, `data:`,
//! comments, CRLF/LF, multi-line data). Fed with arbitrary byte chunks.

#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            self.process_line(&line, &mut out);
        }
        out
    }

    /// Flush a trailing event that was not terminated by a blank line.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if !self.buf.is_empty() {
            let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buf)).into_owned();
            for l in rest.lines() {
                self.process_line(l, &mut out);
            }
        }
        self.process_line("", &mut out);
        out
    }

    fn process_line(&mut self, line: &str, out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            if !self.data.is_empty() || self.event.is_some() {
                out.push(SseEvent { event: self.event.take(), data: self.data.join("\n") });
                self.data.clear();
            }
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.find(':') {
            Some(i) => {
                let v = &line[i + 1..];
                (&line[..i], v.strip_prefix(' ').unwrap_or(v))
            }
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_split_chunks_and_crlf() {
        let mut p = SseParser::new();
        let mut ev = p.feed(b"event: a\r\ndata: {\"x\"");
        assert!(ev.is_empty());
        ev.extend(p.feed(b":1}\r\n\r\n: comment\ndata: line1\ndata: line2\n\n"));
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].event.as_deref(), Some("a"));
        assert_eq!(ev[0].data, "{\"x\":1}");
        assert_eq!(ev[1].data, "line1\nline2");
    }

    #[test]
    fn finish_flushes_unterminated() {
        let mut p = SseParser::new();
        assert!(p.feed(b"data: [DONE]").is_empty());
        let ev = p.finish();
        assert_eq!(ev[0].data, "[DONE]");
    }

    #[test]
    fn handles_multibyte_split() {
        let mut p = SseParser::new();
        let s = "data: 你好\n\n".as_bytes();
        let mut ev = p.feed(&s[..8]);
        ev.extend(p.feed(&s[8..]));
        assert_eq!(ev[0].data, "你好");
    }
}
