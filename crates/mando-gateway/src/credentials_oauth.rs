//! OAuth token helpers.

/// Decode the `exp` claim from a JWT access token.
/// Strips the `sk-ant-si-` prefix if present, then base64-decodes the payload segment.
/// Returns `exp` as Unix milliseconds, or `None` if decoding fails.
pub(crate) fn decode_jwt_expiry(token: &str) -> Option<i64> {
    use base64::Engine;

    let jwt = token.strip_prefix("sk-ant-si-").unwrap_or(token);
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    let exp_secs = value.get("exp")?.as_i64()?;
    Some(exp_secs * 1000)
}
