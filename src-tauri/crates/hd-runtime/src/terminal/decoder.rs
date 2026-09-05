//! Incremental UTF-8 decoding for terminal output.
//!
//! A pty read ends wherever the pipe buffer ends, which can be in the middle of
//! a multi-byte character — and a terminal that paints half of 中 and the other
//! half in the next event is a terminal that flickers. The decoder holds the
//! incomplete sequence back and hands the frontend only whole characters, which
//! is what makes the Rust side the only place decoding has to be right.

/// One decoder, for one session's lifetime.
#[derive(Default)]
pub struct Decoder {
    /// Bytes of an unfinished sequence, waiting for their neighbours.
    pending: Vec<u8>,
}

/// Bytes a sequence still needs, given its lead byte — or `None` for an
/// impossible lead (`0b10…`–`0b11111…`), which is dropped rather than held.
fn expected_length(lead: u8) -> Option<usize> {
    match lead {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode whatever arrived, returning the whole characters it completes.
    pub fn feed(&mut self, bytes: &[u8]) -> String {
        let mut text = String::with_capacity(bytes.len());
        let mut index = 0;

        // What an earlier read left behind completes here, or the whole held
        // sequence is junk and goes — an invalid byte must not poison every
        // character after it.
        if !self.pending.is_empty() {
            let need = expected_length(self.pending[0]);
            match need {
                Some(need) if self.pending.len() + (bytes.len() - index) >= need => {
                    let missing = need - self.pending.len();
                    let mut sequence = std::mem::take(&mut self.pending);
                    sequence.extend_from_slice(&bytes[..missing]);
                    index = missing;
                    match std::str::from_utf8(&sequence) {
                        Ok(decoded) => text.push_str(decoded),
                        Err(_) => text.push('\u{fffd}'),
                    }
                }
                Some(_) => {
                    // Still incomplete: keep holding.
                    self.pending.extend_from_slice(bytes);
                    return text;
                }
                None => {
                    self.pending.clear();
                }
            }
        }

        while index < bytes.len() {
            let need = expected_length(bytes[index]);
            let available = bytes.len() - index;
            match need {
                Some(need) if available >= need => {
                    match std::str::from_utf8(&bytes[index..index + need]) {
                        Ok(decoded) => text.push_str(decoded),
                        Err(_) => text.push('\u{fffd}'),
                    }
                    index += need;
                }
                Some(_) => {
                    // The read stopped mid-character: hold the rest for next time.
                    self.pending.extend_from_slice(&bytes[index..]);
                    break;
                }
                None => {
                    // A stray continuation or impossible lead. One replacement,
                    // and then the next byte gets its own chance.
                    text.push('\u{fffd}');
                    index += 1;
                }
            }
        }
        text
    }

    /// What an ended stream still held. Called once, at close.
    pub fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        self.pending.clear();
        '\u{fffd}'.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::Decoder;

    #[test]
    fn a_character_split_across_reads_arrives_whole() {
        let mut decoder = Decoder::new();
        let bytes = "中文".as_bytes();

        assert_eq!(decoder.feed(&bytes[..2]), "", "half of 中 is not paintable");
        // The second read carries the tail of 中 *and* all of 文: everything
        // that is whole by the time it arrives must paint at once.
        assert_eq!(decoder.feed(&bytes[2..]), "中文");
    }

    #[test]
    fn ascii_between_splits_is_not_delayed() {
        let mut decoder = Decoder::new();
        let bytes = "a中b".as_bytes();

        assert_eq!(decoder.feed(&bytes[..2]), "a");
        assert_eq!(decoder.feed(&bytes[2..]), "中b");
    }

    #[test]
    fn an_impossible_lead_becomes_one_replacement_not_a_held_grudge() {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(&[0xff, b'a']), "\u{fffd}a");
        assert_eq!(decoder.feed(b"b"), "b", "the decoder keeps working");
    }

    #[test]
    fn a_truncated_tail_is_reported_when_the_stream_ends() {
        let mut decoder = Decoder::new();
        let bytes = "中".as_bytes();
        assert_eq!(decoder.feed(&bytes[..1]), "");
        assert_eq!(decoder.finish(), "\u{fffd}");
        assert_eq!(decoder.finish(), "", "finishing twice invents nothing");
    }
}
