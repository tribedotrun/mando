//! Shared parsing helpers for PR evidence sections and embedded media tags.

/// Return the markdown sections that are intended to hold runtime evidence.
///
/// We only trust media and code fences that appear under explicit evidence headings,
/// so unrelated badges or integration images elsewhere in the PR body do not satisfy
/// the evidence gate or consume the evidence download budget.
pub(crate) fn evidence_sections(body: &str) -> Vec<&str> {
    let mut sections = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut current_level = 0usize;
    let mut offset = 0usize;

    for line in body.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();

        let Some((level, is_evidence)) = classify_heading(line) else {
            continue;
        };

        if let Some(start) = current_start {
            if level <= current_level {
                sections.push(&body[start..line_start]);
                current_start = None;
                current_level = 0;
            } else {
                continue;
            }
        }

        if is_evidence {
            current_start = Some(line_start);
            current_level = level;
        }
    }

    if let Some(start) = current_start {
        sections.push(&body[start..]);
    }

    sections
}

/// Extract all `<img ... src=...>` URLs from the provided HTML-ish snippet.
///
/// Matching is case-insensitive for tag and attribute names, but exact for the
/// returned URL content.
pub(crate) fn html_img_src_urls(input: &str) -> Vec<&str> {
    let mut urls = Vec::new();
    let mut rest = input;

    while let Some(tag_start) = find_img_tag(rest) {
        rest = &rest[tag_start + 4..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..tag_end];
        if let Some(url) = extract_html_attr(tag, "src") {
            urls.push(url);
        }
        rest = &rest[tag_end + 1..];
    }

    urls
}

fn classify_heading(line: &str) -> Option<(usize, bool)> {
    let trimmed = line.trim();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level == 0 {
        return None;
    }

    let heading = trimmed[level..].trim().to_ascii_lowercase();
    if heading.is_empty() {
        return None;
    }

    Some((level, is_evidence_heading(&heading)))
}

fn is_evidence_heading(heading: &str) -> bool {
    matches!(
        heading,
        "after" | "evidence" | "visual evidence" | "before / after" | "before/after"
    )
}

fn find_img_tag(input: &str) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut index = 0usize;

    while let Some(relative) = input[index..].find('<') {
        let start = index + relative;
        let after_lt = start + 1;
        let after_img = after_lt.checked_add(3)?;
        if after_img > input.len() {
            return None;
        }
        if !bytes[after_lt..after_img].eq_ignore_ascii_case(b"img") {
            index = after_lt;
            continue;
        }

        let boundary = bytes.get(after_img).copied();
        if matches!(boundary, None | Some(b'>') | Some(b'/'))
            || boundary.is_some_and(|b| b.is_ascii_whitespace())
        {
            return Some(start);
        }

        index = after_lt;
    }

    None
}

fn extract_html_attr<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let bytes = tag.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let name_start = index;
        while index < bytes.len() && is_attr_name_char(bytes[index]) {
            index += 1;
        }
        if name_start == index {
            index += 1;
            continue;
        }

        let name = &tag[name_start..index];

        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            continue;
        }

        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let quote = bytes[index];
        let (value_start, value_end) = if quote == b'"' || quote == b'\'' {
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            (start, index)
        } else {
            let start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'/'
            {
                index += 1;
            }
            (start, index)
        };

        if name.eq_ignore_ascii_case(attr) {
            return Some(tag[value_start..value_end].trim());
        }
    }

    None
}

fn is_attr_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_evidence_scoped_sections() {
        let body = r#"## Summary
No evidence yet.

## Evidence
![fix](https://example.com/fix.png)

## Footer
<img src="https://example.com/badge.png" alt="badge" />
"#;
        let sections = evidence_sections(body);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].contains("fix.png"));
        assert!(!sections[0].contains("badge.png"));
    }

    #[test]
    fn html_img_urls_are_case_insensitive() {
        let urls = html_img_src_urls(
            r#"<IMG SRC="https://github.com/user-attachments/assets/demo" alt="proof" />"#,
        );
        assert_eq!(
            urls,
            vec!["https://github.com/user-attachments/assets/demo"]
        );
    }

    #[test]
    fn html_img_urls_ignore_data_src() {
        let urls = html_img_src_urls(
            r#"<img data-src="https://wrong.example.com/wrong.png" src="https://right.example.com/right.png">"#,
        );
        assert_eq!(urls, vec!["https://right.example.com/right.png"]);
    }

    #[test]
    fn html_img_urls_ignore_non_ascii_after_angle_bracket() {
        let urls =
            html_img_src_urls("<🎉\n<img src=\"https://github.com/user-attachments/assets/demo\">");
        assert_eq!(
            urls,
            vec!["https://github.com/user-attachments/assets/demo"]
        );
    }
}
