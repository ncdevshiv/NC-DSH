//! Document chunking: fixed-size (with overlap) and sentence-based
//! strategies (PRD §3.8.1).

/// A chunk of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub text: String,
    /// Character offset of the chunk start in the source document.
    pub start: usize,
    pub index: usize,
}

/// Chunking strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkingStrategy {
    /// Fixed-size chunks with optional overlap (in characters).
    Fixed { size: usize, overlap: usize },
    /// Split on sentence boundaries (`.`, `!`, `?`), keeping chunks under
    /// `max_size` characters.
    Sentence { max_size: usize },
}

/// Splits a document into chunks per the strategy.
pub fn chunk_document(document: &str, strategy: ChunkingStrategy) -> Vec<Chunk> {
    match strategy {
        ChunkingStrategy::Fixed { size, overlap } => {
            let size = size.max(1);
            let overlap = overlap.min(size.saturating_sub(1));
            let mut chunks = Vec::new();
            let mut index = 0usize;
            let mut start = 0usize;
            while start < document.len() {
                // Snap the window to character boundaries so slicing never
                // panics on multi-byte UTF-8.
                let mut end = (start + size).min(document.len());
                while end > start && !document.is_char_boundary(end) {
                    end -= 1;
                }
                if end == start {
                    // `size` is smaller than the width of the character at
                    // `start`; widen the window just past that character so
                    // the slice stays valid and scanning makes progress.
                    // Without this the empty window could never advance.
                    end = start + 1;
                    while !document.is_char_boundary(end) {
                        end += 1;
                    }
                }
                let text = &document[start..end];
                // Skip windows carrying no coverable content. Must match the
                // coverage definition (ASCII whitespace): `str::trim` also
                // drops e.g. U+000B VERTICAL TAB and NBSP, which are NOT
                // ASCII whitespace, so using `trim` here silently dropped
                // bytes the caller still expects chunks to cover.
                if text.bytes().all(|b| b.is_ascii_whitespace()) {
                    // Skip whitespace-only windows — they carry no content —
                    // but keep scanning so the rest of the document is not
                    // silently dropped. `end > start` always holds here, so
                    // this path strictly advances.
                    if end >= document.len() {
                        break;
                    }
                    start = end;
                    continue;
                }
                chunks.push(Chunk {
                    text: text.to_string(),
                    start,
                    index,
                });
                index += 1;
                if end >= document.len() {
                    break;
                }
                // Overlap rewinds the next start, but never behind the start
                // of the chunk just emitted: UTF-8 snapping above can leave
                // the effective window shorter than `overlap`, and rewinding
                // to `<= start` would re-emit the same chunk forever.
                let mut next_start = end.saturating_sub(overlap);
                if next_start <= start {
                    next_start = end;
                }
                // Snap forward (never backward) to a character boundary, so
                // `start` strictly increases on every iteration and the loop
                // is guaranteed to terminate.
                while !document.is_char_boundary(next_start) {
                    next_start += 1;
                }
                start = next_start;
            }
            chunks
        }
        ChunkingStrategy::Sentence { max_size } => {
            let max_size = max_size.max(1);
            let mut chunks = Vec::new();
            let mut current = String::new();
            let mut current_start = 0usize;
            let mut index = 0usize;

            // Track the byte offset of each sentence as it is produced, so
            // chunk starts point at the true position in the source.
            let mut offset = 0usize;
            for sentence in split_sentences(document) {
                let sentence_start = offset;
                offset += sentence.len();
                if current.len() + sentence.len() > max_size && !current.is_empty() {
                    chunks.push(Chunk {
                        text: std::mem::take(&mut current),
                        start: current_start,
                        index,
                    });
                    index += 1;
                    current_start = sentence_start;
                }
                current.push_str(&sentence);
            }
            if !current.trim().is_empty() {
                chunks.push(Chunk {
                    text: current,
                    start: current_start,
                    index,
                });
            }
            chunks
        }
    }
}

/// Splits text into sentences on `.`, `!`, `?` boundaries (keeping the
/// terminator), handling common abbreviations conservatively (a terminator
/// followed by whitespace + capital letter or end-of-input).
fn split_sentences(document: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = document.chars().peekable();

    while let Some(ch) = chars.next() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            // Look ahead: sentence ends if the next non-space char is a
            // capital letter or there is no more input.
            let mut ahead = chars.clone().peekable();
            let mut next_meaningful = None;
            for candidate in ahead.by_ref() {
                if !candidate.is_whitespace() {
                    next_meaningful = Some(candidate);
                    break;
                }
            }
            let ends = match next_meaningful {
                None => true,
                Some(c) => c.is_uppercase() || c.is_ascii_digit(),
            };
            if ends {
                // Include trailing whitespace with the sentence.
                while let Some(&ws) = chars.peek() {
                    if ws.is_whitespace() {
                        current.push(ws);
                        chars.next();
                    } else {
                        break;
                    }
                }
                sentences.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.trim().is_empty() {
        sentences.push(current);
    }
    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_chunking_respects_size_and_overlap() {
        let doc = "abcdefghijklmnopqrstuvwxyz";
        let chunks = chunk_document(
            doc,
            ChunkingStrategy::Fixed {
                size: 10,
                overlap: 3,
            },
        );
        assert_eq!(chunks.len(), 4, "{chunks:?}");
        assert_eq!(chunks[0].text, "abcdefghij");
        assert_eq!(chunks[1].text, "hijklmnopq");
        assert_eq!(chunks[0].start, 0);
        assert!(chunks[1].start > 0);
        // The whole document is covered (with overlap).
        assert!(chunks.last().unwrap().text.ends_with('z'));
    }

    #[test]
    fn fixed_chunking_handles_short_documents() {
        let chunks = chunk_document(
            "hi",
            ChunkingStrategy::Fixed {
                size: 10,
                overlap: 0,
            },
        );
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "hi");
    }

    /// Regression: with multi-byte text, UTF-8 boundary snapping can leave a
    /// window shorter than the requested overlap (or even empty when `size`
    /// is below the character width). Chunking must still advance strictly
    /// monotonically, terminate quickly, and cover every non-whitespace byte.
    #[test]
    fn fixed_chunking_multibyte_large_overlap_terminates_and_covers() {
        let doc = "日本語のテキスト 🦀🚀 絵文字😀 混合";
        for size in [1usize, 2, 3, 5] {
            for overlap in [0usize, 1, 4, 31] {
                let chunks = chunk_document(doc, ChunkingStrategy::Fixed { size, overlap });
                assert!(!chunks.is_empty(), "size {size} overlap {overlap}");
                for pair in chunks.windows(2) {
                    assert!(
                        pair[0].start < pair[1].start,
                        "starts must strictly increase (size {size}, overlap {overlap})"
                    );
                }
                let mut covered = vec![false; doc.len()];
                for chunk in &chunks {
                    assert!(
                        doc.is_char_boundary(chunk.start)
                            && doc[chunk.start..].starts_with(chunk.text.as_str()),
                        "chunk at invalid offset (size {size}, overlap {overlap}): {chunk:?}"
                    );
                    for slot in &mut covered[chunk.start..chunk.start + chunk.text.len()] {
                        *slot = true;
                    }
                }
                for (i, b) in doc.as_bytes().iter().enumerate() {
                    if !b.is_ascii_whitespace() {
                        assert!(
                            covered[i],
                            "byte {i} uncovered (size {size}, overlap {overlap})"
                        );
                    }
                }
            }
        }
    }

    /// Regression: `size` smaller than one character width must not stall on
    /// an empty snapped window; each non-whitespace character becomes a
    /// chunk and scanning always advances.
    #[test]
    fn fixed_chunking_size_below_char_width_advances() {
        let doc = "héllo🌍!";
        let chunks = chunk_document(
            doc,
            ChunkingStrategy::Fixed {
                size: 1,
                overlap: 0,
            },
        );
        let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(joined, doc, "one chunk per character, content preserved");
        for pair in chunks.windows(2) {
            assert!(pair[0].start < pair[1].start);
        }
    }

    /// Regression: U+000B VERTICAL TAB is Unicode whitespace but not ASCII
    /// whitespace; windows containing it must be emitted so their bytes stay
    /// covered by chunking.
    #[test]
    fn fixed_chunking_keeps_non_ascii_whitespace_covered() {
        let chunks = chunk_document(
            "\u{b}x",
            ChunkingStrategy::Fixed {
                size: 1,
                overlap: 0,
            },
        );
        assert_eq!(chunks.len(), 2, "{chunks:?}");
        assert_eq!(chunks[0].text, "\u{b}");
        assert_eq!(chunks[1].text, "x");
    }

    #[test]
    fn sentence_chunking_splits_on_boundaries() {
        let doc = "First sentence. Second sentence! Third? Fourth.";
        // With a large max_size the whole document is one chunk.
        let one = chunk_document(doc, ChunkingStrategy::Sentence { max_size: 200 });
        assert_eq!(one.len(), 1);
        assert!(one[0].text.contains("Fourth."));
        // With a small max_size, chunks split at sentence boundaries.
        let many = chunk_document(doc, ChunkingStrategy::Sentence { max_size: 17 });
        assert!(many.len() >= 3, "split at boundaries: {many:?}");
        assert!(many[0].text.contains("First sentence"), "{:?}", many[0]);
        assert!(many[0].text.contains('.'), "keeps the terminator");
    }

    #[test]
    fn sentence_chunking_bounds_chunk_size() {
        let doc = "Alpha. ".repeat(60);
        let chunks = chunk_document(&doc, ChunkingStrategy::Sentence { max_size: 100 });
        assert!(
            chunks.iter().all(|c| c.text.len() <= 100),
            "all chunks bounded"
        );
        assert!(chunks.len() >= 2, "multiple chunks: {}", chunks.len());
    }
}
