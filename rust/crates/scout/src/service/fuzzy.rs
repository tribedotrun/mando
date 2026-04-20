//! Fuzzy text matching for scout search with typo tolerance.

/// Score how well `query` matches `text`. Returns 0.0 (no match) to 1.0 (exact).
///
/// Matching tiers:
/// 1. Exact case-insensitive substring → 1.0
/// 2. Every query word is a substring of some text word → 0.9
/// 3. Word-level edit distance within tolerance → 0.5–0.85 based on distance
/// 4. No match → 0.0
pub fn fuzzy_score(query: &str, text: &str) -> f64 {
    if query.is_empty() || text.is_empty() {
        return 0.0;
    }

    let q = query.to_lowercase();
    let t = text.to_lowercase();

    // Tier 1: exact substring
    if t.contains(&q) {
        return 1.0;
    }

    // Split into words
    let query_words: Vec<&str> = q.split_whitespace().collect();
    let text_words: Vec<&str> = t
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1)
        .collect();

    if query_words.is_empty() || text_words.is_empty() {
        return 0.0;
    }

    // Tier 2: every query word is a substring of some text word
    let all_substr = query_words
        .iter()
        .all(|qw| text_words.iter().any(|tw| tw.contains(qw)));
    if all_substr {
        return 0.9;
    }

    // Tier 3: word-level edit distance
    let avg = query_words
        .iter()
        .map(|qw| {
            text_words
                .iter()
                .map(|tw| word_similarity(qw, tw))
                .fold(0.0_f64, f64::max)
        })
        .sum::<f64>()
        / query_words.len() as f64;

    if avg >= 0.5 {
        avg * 0.85
    } else {
        0.0
    }
}

/// Similarity between two words: 1.0 = identical, 0.0 = completely different.
fn word_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // Substring match (query word inside text word)
    if b.contains(a) {
        return 0.95;
    }

    let dist = levenshtein(a, b);
    let max_len = a.len().max(b.len());

    // Adaptive threshold: allow more edits for longer words
    let max_edits = match max_len {
        0..=3 => 1,
        4..=6 => 2,
        _ => 3,
    };

    if dist > max_edits {
        return 0.0;
    }

    1.0 - (dist as f64 / max_len as f64)
}

/// Levenshtein edit distance (single-row DP).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = dp[0];
        dp[0] = i;
        for j in 1..=b.len() {
            let temp = dp[j];
            dp[j] = if a[i - 1] == b[j - 1] {
                prev
            } else {
                1 + prev.min(dp[j]).min(dp[j - 1])
            };
            prev = temp;
        }
    }
    dp[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_substring() {
        assert_eq!(fuzzy_score("stripe", "Stripe's API Versioning"), 1.0);
    }

    #[test]
    fn url_match() {
        assert_eq!(
            fuzzy_score("openai", "https://openai.com/blog/scaling"),
            1.0
        );
    }

    #[test]
    fn typo_one_char() {
        let score = fuzzy_score("strpe", "Stripe's API Versioning");
        assert!(score > 0.4, "expected match for 'strpe', got {score}");
    }

    #[test]
    fn typo_swap() {
        let score = fuzzy_score("stirpe", "Stripe's API Versioning");
        assert!(score > 0.4, "expected match for 'stirpe', got {score}");
    }

    #[test]
    fn multi_word_partial() {
        let score = fuzzy_score("stripe api", "Stripe's API Versioning");
        assert!(
            score > 0.8,
            "expected high score for 'stripe api', got {score}"
        );
    }

    #[test]
    fn no_match() {
        assert_eq!(fuzzy_score("kubernetes", "Stripe's API Versioning"), 0.0);
    }

    #[test]
    fn empty_query() {
        assert_eq!(fuzzy_score("", "some text"), 0.0);
    }

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("stripe", "strpe"), 1);
        assert_eq!(levenshtein("abc", "abc"), 0);
    }
}
