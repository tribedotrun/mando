pub fn now_rfc3339() -> String {
    // Formatting a well-known RFC 3339 UTC timestamp is infallible; the
    // error path is unreachable in practice. `unrecoverable!` is the right
    // failure channel if the formatter ever changes behaviour.
    match time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339) {
        Ok(s) => s,
        Err(e) => crate::unrecoverable!("RFC 3339 format of UTC time failed", e),
    }
}
