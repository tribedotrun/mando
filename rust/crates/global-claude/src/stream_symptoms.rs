//! Typed symptoms detected in CC stream output.
//!
//! The Claude CLI emits human-readable error text into its stream when certain
//! server-side conditions fire (rate limit, image dimension limit, etc.).
//! Callers that need to branch on these symptoms should downcast from a typed
//! enum instead of each grep'ing the raw text themselves.
//!
//! This module owns the match-text knowledge — downstream modules only see
//! [`CcStreamSymptom`].

/// A known, actionable condition detected in the CC stream output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CcStreamSymptom {
    /// The Anthropic API rejected an image attachment because its dimensions
    /// exceed the 2000px limit. Emitted as free-form text in the stream
    /// result envelope; captain uses this to nudge the worker to resize.
    ImageDimensionLimit,
}

/// Inspect `stream_tail` for known CC-server symptoms.
pub fn detect_cc_stream_symptom(stream_tail: &str) -> Option<CcStreamSymptom> {
    if stream_tail.contains("exceeds the dimension limit") && stream_tail.contains("2000px") {
        return Some(CcStreamSymptom::ImageDimensionLimit);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_image_dimension_limit() {
        let tail = "API Error: image exceeds the dimension limit of 2000px × 2000px";
        assert_eq!(
            detect_cc_stream_symptom(tail),
            Some(CcStreamSymptom::ImageDimensionLimit)
        );
    }

    #[test]
    fn rejects_unrelated_text() {
        assert_eq!(detect_cc_stream_symptom("all good"), None);
    }

    #[test]
    fn requires_both_phrases() {
        // Missing the 2000px marker should not trigger — avoids matching
        // a generic "dimension limit" phrase from a different product.
        assert_eq!(
            detect_cc_stream_symptom("exceeds the dimension limit"),
            None
        );
    }
}
