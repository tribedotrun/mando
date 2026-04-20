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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemStatus, Task};

    /// Count items by status.
    fn status_counts(items: &[Task]) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for item in items {
            *counts.entry(item.status.to_string()).or_insert(0) += 1;
        }
        counts
    }

    #[test]
    fn status_counts_basic() {
        let mut a = Task::new("A");
        a.status = ItemStatus::New;
        let mut b = Task::new("B");
        b.status = ItemStatus::New;
        let mut c = Task::new("C");
        c.status = ItemStatus::InProgress;

        let counts = status_counts(&[a, b, c]);
        assert_eq!(counts.get("new"), Some(&2));
        assert_eq!(counts.get("in-progress"), Some(&1));
    }

    #[test]
    fn format_summary() {
        let mut counts = std::collections::HashMap::new();
        counts.insert("new".into(), 2);
        counts.insert("in-progress".into(), 1);
        let s = format_status_summary(&counts);
        assert!(s.contains("in-progress=1"));
        assert!(s.contains("new=2"));
    }
}
