pub fn parse_i64_id(id: &str, label: &str) -> Result<i64, String> {
    id.parse::<i64>()
        .map_err(|_| format!("invalid {label} ID: {id}"))
}

pub fn slugify(title: &str, max_len: usize) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut prev_hyphen = true;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !prev_hyphen {
            slug.push('-');
            prev_hyphen = true;
        }
    }
    let trimmed = slug.trim_end_matches('-');
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }
    trimmed[..max_len].trim_end_matches('-').to_string()
}
