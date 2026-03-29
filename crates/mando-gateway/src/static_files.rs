//! Static file serving — images and web dashboard assets.

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use serde_json::json;

/// GET /api/images/{filename} — serve a file from ~/.mando/images/.
pub(crate) async fn get_image(Path(filename): Path<String>) -> impl IntoResponse {
    // Sanitize: prevent path traversal.
    let safe_name: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();

    if safe_name.is_empty() || safe_name.contains("..") {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            json!({"error": "invalid filename"})
                .to_string()
                .into_bytes(),
        );
    }

    let images_dir = mando_config::images_dir();
    let path = images_dir.join(&safe_name);

    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let content_type = guess_content_type(&safe_name);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type)],
                bytes,
            )
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            json!({"error": "image not found"}).to_string().into_bytes(),
        ),
    }
}

/// Guess MIME type from file extension.
fn guess_content_type(filename: &str) -> &'static str {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_detection() {
        assert_eq!(guess_content_type("photo.png"), "image/png");
        assert_eq!(guess_content_type("photo.JPG"), "image/jpeg");
        assert_eq!(guess_content_type("icon.svg"), "image/svg+xml");
        assert_eq!(guess_content_type("file.bin"), "application/octet-stream");
    }
}
