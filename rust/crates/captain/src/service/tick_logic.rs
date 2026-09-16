//! Tick phase orchestration logic — pure decision helpers.

/// Format status counts for logging.
pub(crate) fn format_status_summary(counts: &std::collections::HashMap<String, usize>) -> String {
    let mut pairs: Vec<_> = counts.iter().collect();
    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
    pairs
        .iter()
        .map(|(s, c)| format!("{}={}", s, c))
        .collect::<Vec<_>>()
        .join(", ")
}
