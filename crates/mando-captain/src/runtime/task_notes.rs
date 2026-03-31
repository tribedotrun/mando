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

pub(crate) fn append_labeled_prompt(prompt: &mut String, label: &str, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    prompt.push_str("\n\n");
    prompt.push_str(label);
    prompt.push_str(": ");
    prompt.push_str(text);
}

#[cfg(test)]
mod tests {
    use super::{append_labeled_prompt, append_tagged_note, tagged_note};

    #[test]
    fn tagged_note_ignores_blank_text() {
        assert_eq!(tagged_note("Human answer", "   "), None);
    }

    #[test]
    fn append_tagged_note_appends_to_existing_context() {
        let updated =
            append_tagged_note(Some("Existing context"), "Human answer", "Need more logs")
                .expect("note should be added");
        assert_eq!(updated, "Existing context\n\n[Human answer] Need more logs");
    }

    #[test]
    fn append_labeled_prompt_ignores_blank_text() {
        let mut prompt = String::from("Base prompt");
        append_labeled_prompt(&mut prompt, "Human provided additional details", "   ");
        assert_eq!(prompt, "Base prompt");
    }

    #[test]
    fn append_labeled_prompt_adds_section() {
        let mut prompt = String::from("Base prompt");
        append_labeled_prompt(
            &mut prompt,
            "Human provided additional details",
            "it's in the login page",
        );
        assert_eq!(
            prompt,
            "Base prompt\n\nHuman provided additional details: it's in the login page"
        );
    }
}
