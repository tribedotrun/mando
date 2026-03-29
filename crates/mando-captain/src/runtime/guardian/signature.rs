//! Incident signature — stable hash for deduplicating similar incidents.

use std::sync::LazyLock;

use regex::Regex;

static DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());

/// Normalize digits→`<n>`, collapse whitespace, return FNV-1a[:16] hex.
pub(crate) fn incident_signature(text: &str) -> String {
    let lower = text.to_lowercase();
    let normalized = DIGIT_RE.replace_all(&lower, "<n>");
    let collapsed: String = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let end = collapsed.len().min(500);
    let hash = fnv1a(&collapsed.as_bytes()[..end]);
    format!("{hash:016x}")
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_signature() {
        let sig1 = incident_signature("error at line 42: connection refused");
        let sig2 = incident_signature("error at line 99: connection refused");
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 16);
    }

    #[test]
    fn different_messages_differ() {
        let sig1 = incident_signature("timeout connecting to database");
        let sig2 = incident_signature("file not found: config.json");
        assert_ne!(sig1, sig2);
    }
}
