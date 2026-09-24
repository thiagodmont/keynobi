use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Maximum bytes kept from a single line of subprocess output.
///
/// Gradle, adb, and logcat lines are almost always under 1 KiB, and the longest
/// legitimate ones (compiler diagnostics with long classpaths, JSON log bodies)
/// stay within a few KiB. 64 KiB leaves wide headroom for those while bounding
/// memory when a tool prints a multi-megabyte line or never prints a newline.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// Reads lines like `tokio::io::Lines`, but keeps at most `MAX_LINE_BYTES` of
/// each line in memory. The rest of an overlong line is discarded up to the next
/// newline and the returned line ends with `… [truncated N bytes]`.
///
/// Invalid UTF-8 is replaced with U+FFFD instead of returning an error, so one
/// bad byte cannot end the stream.
pub struct CappedLines<R> {
    reader: R,
    buf: Vec<u8>,
    max_bytes: usize,
    truncated_bytes: usize,
    ends_with_cr: bool,
}

impl<R: AsyncBufRead + Unpin> CappedLines<R> {
    pub fn new(reader: R) -> Self {
        Self::with_max_bytes(reader, MAX_LINE_BYTES)
    }

    fn with_max_bytes(reader: R, max_bytes: usize) -> Self {
        Self {
            reader,
            buf: Vec::new(),
            max_bytes,
            truncated_bytes: 0,
            ends_with_cr: false,
        }
    }

    /// Returns the next line without its `\n` or `\r\n`, or `None` at EOF.
    ///
    /// Cancel safe: partial lines are kept in `self` between calls, and input
    /// is consumed only after it has been recorded, so this can be used as a
    /// `tokio::select!` branch without losing data.
    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                if self.buf.is_empty() && self.truncated_bytes == 0 {
                    return Ok(None);
                }
                return Ok(Some(self.take_line(false)));
            }

            let newline = available.iter().position(|&b| b == b'\n');
            let chunk = &available[..newline.unwrap_or(available.len())];
            let keep = chunk.len().min(self.max_bytes - self.buf.len());
            self.buf.extend_from_slice(&chunk[..keep]);
            self.truncated_bytes += chunk.len() - keep;
            if let Some(&last) = chunk.last() {
                self.ends_with_cr = last == b'\r';
            }
            let consumed = newline.map_or(chunk.len(), |i| i + 1);
            self.reader.consume(consumed);

            if newline.is_some() {
                return Ok(Some(self.take_line(true)));
            }
        }
    }

    fn take_line(&mut self, at_newline: bool) -> String {
        let mut bytes = std::mem::take(&mut self.buf);
        let mut truncated = std::mem::take(&mut self.truncated_bytes);
        if std::mem::take(&mut self.ends_with_cr) && at_newline {
            // Once truncation starts every later byte is discarded, so the
            // `\r` is either the last kept byte or the last discarded one.
            if truncated > 0 {
                truncated -= 1;
            } else {
                bytes.pop();
            }
        }
        if truncated == 0 {
            return String::from_utf8(bytes)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
        }

        // Do not end on a partial character split by the cap.
        if let Err(e) = std::str::from_utf8(&bytes) {
            if e.error_len().is_none() {
                truncated += bytes.len() - e.valid_up_to();
                bytes.truncate(e.valid_up_to());
            }
        }
        let mut line = String::from_utf8_lossy(&bytes).into_owned();
        line.push_str(&format!("… [truncated {truncated} bytes]"));
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncWriteExt, BufReader};

    /// Reads all lines, forcing small buffer refills so lines span chunks.
    async fn read_all(input: &[u8], max_bytes: usize) -> Vec<String> {
        let reader = BufReader::with_capacity(7, input);
        let mut lines = CappedLines::with_max_bytes(reader, max_bytes);
        let mut out = Vec::new();
        while let Some(line) = lines.next_line().await.unwrap() {
            out.push(line);
        }
        out
    }

    #[tokio::test]
    async fn lines_under_the_cap_are_unchanged() {
        let lines = read_all(b"one\ntwo\r\n\nthree", 16).await;
        assert_eq!(lines, vec!["one", "two", "", "three"]);
    }

    #[tokio::test]
    async fn a_line_exactly_at_the_cap_is_not_truncated() {
        let lines = read_all(b"0123456789\nnext\n", 10).await;
        assert_eq!(lines, vec!["0123456789", "next"]);
    }

    #[tokio::test]
    async fn crlf_at_the_cap_is_not_counted_as_truncation() {
        let lines = read_all(b"0123456789\r\nnext\n", 10).await;
        assert_eq!(lines, vec!["0123456789", "next"]);
    }

    #[tokio::test]
    async fn an_overlong_line_is_truncated_and_the_next_line_is_intact() {
        let mut input = vec![b'a'; 1_000];
        input.extend_from_slice(b"\nnext\n");
        let lines = read_all(&input, 10).await;
        assert_eq!(lines, vec!["aaaaaaaaaa… [truncated 990 bytes]", "next"]);
    }

    #[tokio::test]
    async fn one_byte_over_the_cap_is_truncated() {
        let lines = read_all(b"0123456789X\r\nnext\n", 10).await;
        assert_eq!(lines, vec!["0123456789… [truncated 1 bytes]", "next"]);
    }

    #[tokio::test]
    async fn an_overlong_line_without_a_newline_is_returned_at_eof() {
        let lines = read_all(&[b'a'; 50], 10).await;
        assert_eq!(lines, vec!["aaaaaaaaaa… [truncated 40 bytes]"]);
    }

    #[tokio::test]
    async fn the_default_cap_bounds_a_multi_megabyte_line() {
        let mut input = vec![b'x'; 3 * 1024 * 1024];
        input.extend_from_slice(b"\nnext\n");
        let mut lines = CappedLines::new(BufReader::new(&input[..]));
        let first = lines.next_line().await.unwrap().unwrap();
        let dropped = input.len() - 6 - MAX_LINE_BYTES;
        assert!(first.starts_with(&"x".repeat(MAX_LINE_BYTES)));
        assert!(first.ends_with(&format!("… [truncated {dropped} bytes]")));
        assert!(first.len() < MAX_LINE_BYTES + 64);
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("next"));
        assert_eq!(lines.next_line().await.unwrap(), None);
    }

    #[tokio::test]
    async fn invalid_utf8_is_replaced_without_ending_the_stream() {
        let lines = read_all(b"bad \xff\xfe byte\nnext\n", 64).await;
        assert_eq!(lines, vec!["bad \u{fffd}\u{fffd} byte", "next"]);
    }

    #[tokio::test]
    async fn truncation_does_not_split_a_multibyte_character() {
        // "é" is 2 bytes; a cap of 5 falls inside the third one.
        let lines = read_all("ééé\nnext\n".as_bytes(), 5).await;
        assert_eq!(lines, vec!["éé… [truncated 2 bytes]", "next"]);
    }

    #[tokio::test]
    async fn a_cancelled_read_keeps_the_partial_line() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut lines = CappedLines::new(BufReader::new(rx));

        tx.write_all(b"hello ").await.unwrap();
        let pending =
            tokio::time::timeout(std::time::Duration::from_millis(50), lines.next_line()).await;
        assert!(
            pending.is_err(),
            "no newline yet, so the read must still be pending"
        );

        tx.write_all(b"world\n").await.unwrap();
        assert_eq!(
            lines.next_line().await.unwrap().as_deref(),
            Some("hello world")
        );
    }
}
