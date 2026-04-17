use crate::CcConfigBuilder;

pub fn with_credential(
    builder: CcConfigBuilder,
    credential: &Option<(i64, String)>,
) -> CcConfigBuilder {
    if let Some((_id, token)) = credential {
        builder.env("CLAUDE_CODE_OAUTH_TOKEN", token)
    } else {
        builder
    }
}

pub fn credential_id(credential: &Option<(i64, String)>) -> Option<i64> {
    credential.as_ref().map(|c| c.0)
}
