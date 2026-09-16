//! Shared helpers for human-authored captain context and prompt notes.

pub(crate) fn tagged_note(tag: &str, text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(format!("[{tag}] {text}"))
}

pub fn append_tagged_note(existing: Option<&str>, tag: &str, text: &str) -> Option<String> {
    let note = tagged_note(tag, text)?;
    match existing.map(str::trim).filter(|value| !value.is_empty()) {
        Some(existing) => Some(format!("{existing}\n\n{note}")),
        None => Some(note),
    }
}
