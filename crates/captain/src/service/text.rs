//! Pure string utilities.

pub(crate) fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    &s[..s.floor_char_boundary(max)]
}
