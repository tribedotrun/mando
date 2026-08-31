use api_types::{RouteAuth, RouteMethod, RouteTransport};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;
use tracing::debug;

/// Complete, generated metadata for one daemon route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDescriptor {
    pub method: RouteMethod,
    pub path: &'static str,
    pub transport: RouteTransport,
    pub auth: RouteAuth,
    pub params: Option<&'static str>,
    pub query: Option<&'static str>,
    pub body: Option<&'static str>,
    pub response: Option<&'static str>,
    pub event: Option<&'static str>,
}

/// Failure returned by the shared daemon client.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("daemon request failed")]
    Request {
        #[source]
        source: reqwest::Error,
    },
    #[error("daemon returned {status}: {body}")]
    Http { status: StatusCode, body: String },
    #[error("failed to read daemon response")]
    ResponseBody {
        #[source]
        source: reqwest::Error,
    },
    #[error("invalid JSON response")]
    InvalidJson {
        #[source]
        source: reqwest::Error,
    },
}

impl ClientError {
    /// Whether the daemon TCP connection was refused or otherwise unavailable.
    pub fn is_connect(&self) -> bool {
        matches!(self, Self::Request { source } if source.is_connect())
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// Shared HTTP client used by every Rust daemon caller.
#[derive(Clone)]
pub struct GatewayClient {
    client: Client,
    base_url: String,
    token: Option<String>,
    accept_server_error_bodies: bool,
}

impl GatewayClient {
    /// Build a localhost client using Mando's shared reqwest client.
    pub fn localhost(port: u16, token: Option<String>) -> Self {
        Self::with_client(
            format!("http://127.0.0.1:{port}"),
            token,
            (*global_net::shared_client()).clone(),
        )
    }

    /// Build a client with an explicit HTTP implementation.
    pub fn with_client(base_url: impl Into<String>, token: Option<String>, client: Client) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token,
            accept_server_error_bodies: false,
        }
    }

    /// Decode structured response bodies returned with 5xx statuses.
    ///
    /// Health surfaces use this to render degraded daemon details. Other
    /// generated calls retain normal non-success handling.
    pub fn accepting_server_error_bodies(&self) -> Self {
        let mut client = self.clone();
        client.accept_server_error_bodies = true;
        client
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub(crate) async fn get_json<R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
    ) -> Result<R> {
        let request = self.request(route, path);
        self.send_json(route, request).await
    }

    pub(crate) async fn get_json_query<Q: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        query: &Q,
    ) -> Result<R> {
        let request = self.request(route, path).query(query);
        self.send_json(route, request).await
    }

    pub(crate) async fn post_json<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let request = self.request(route, path).json(body);
        self.send_json(route, request).await
    }

    pub(crate) async fn post_json_query<
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
        R: DeserializeOwned,
    >(
        &self,
        route: &RouteDescriptor,
        path: &str,
        query: &Q,
        body: &B,
    ) -> Result<R> {
        let request = self.request(route, path).query(query).json(body);
        self.send_json(route, request).await
    }

    pub(crate) async fn put_json<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let request = self.request(route, path).json(body);
        self.send_json(route, request).await
    }

    pub(crate) async fn patch_json<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let request = self.request(route, path).json(body);
        self.send_json(route, request).await
    }

    pub(crate) async fn delete_json<R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
    ) -> Result<R> {
        let request = self.request(route, path);
        self.send_json(route, request).await
    }

    pub(crate) async fn delete_json_body<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let request = self.request(route, path).json(body);
        self.send_json(route, request).await
    }

    pub(crate) async fn post_multipart<R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<R> {
        let request = self.request(route, path).multipart(form);
        self.send_json(route, request).await
    }

    pub(crate) async fn post_multipart_query<Q: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        query: &Q,
        form: reqwest::multipart::Form,
    ) -> Result<R> {
        let request = self.request(route, path).query(query).multipart(form);
        self.send_json(route, request).await
    }

    pub(crate) async fn get_text_query<Q: Serialize + ?Sized>(
        &self,
        route: &RouteDescriptor,
        path: &str,
        query: &Q,
    ) -> Result<String> {
        let request = self.request(route, path).query(query);
        self.send_text(route, request).await
    }

    pub(crate) async fn get_bytes(&self, route: &RouteDescriptor, path: &str) -> Result<Vec<u8>> {
        let request = self.request(route, path);
        self.send_bytes(route, request).await
    }

    fn request(&self, route: &RouteDescriptor, path: &str) -> RequestBuilder {
        let method = match route.method {
            RouteMethod::Get => reqwest::Method::GET,
            RouteMethod::Post => reqwest::Method::POST,
            RouteMethod::Put => reqwest::Method::PUT,
            RouteMethod::Patch => reqwest::Method::PATCH,
            RouteMethod::Delete => reqwest::Method::DELETE,
        };
        let mut request = self.client.request(method, self.url(path));
        if route.auth == RouteAuth::Protected {
            if let Some(token) = &self.token {
                request = request.bearer_auth(token);
            }
        }
        request
    }

    async fn send_json<R: DeserializeOwned>(
        &self,
        route: &RouteDescriptor,
        request: RequestBuilder,
    ) -> Result<R> {
        let started = std::time::Instant::now();
        let response = request
            .send()
            .await
            .map_err(|source| ClientError::Request { source })?;
        let status = response.status();
        debug!(
            method = route.method.as_http(),
            path = route.path,
            status = %status,
            elapsed_ms = started.elapsed().as_millis(),
            "daemon request"
        );
        if !(status.is_success() || self.accept_server_error_bodies && status.is_server_error()) {
            let body = response
                .text()
                .await
                .map_err(|source| ClientError::ResponseBody { source })?;
            return Err(ClientError::Http { status, body });
        }
        response
            .json()
            .await
            .map_err(|source| ClientError::InvalidJson { source })
    }

    async fn send_text(&self, route: &RouteDescriptor, request: RequestBuilder) -> Result<String> {
        let started = std::time::Instant::now();
        let response = request
            .send()
            .await
            .map_err(|source| ClientError::Request { source })?;
        let status = response.status();
        debug!(
            method = route.method.as_http(),
            path = route.path,
            status = %status,
            elapsed_ms = started.elapsed().as_millis(),
            "daemon request"
        );
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|source| ClientError::ResponseBody { source })?;
            return Err(ClientError::Http { status, body });
        }
        response
            .text()
            .await
            .map_err(|source| ClientError::ResponseBody { source })
    }

    async fn send_bytes(
        &self,
        route: &RouteDescriptor,
        request: RequestBuilder,
    ) -> Result<Vec<u8>> {
        let started = std::time::Instant::now();
        let response = request
            .send()
            .await
            .map_err(|source| ClientError::Request { source })?;
        let status = response.status();
        debug!(
            method = route.method.as_http(),
            path = route.path,
            status = %status,
            elapsed_ms = started.elapsed().as_millis(),
            "daemon request"
        );
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|source| ClientError::ResponseBody { source })?;
            return Err(ClientError::Http { status, body });
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|source| ClientError::ResponseBody { source })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

pub(crate) fn render_path(route: &RouteDescriptor, values: &[(&str, String)]) -> String {
    let mut path = route.path.to_string();
    for (name, value) in values {
        let encoded = urlencoding::encode(value);
        path = path.replace(&format!("{{{name}}}"), encoded.as_ref());
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_escapes_path_parameters() {
        let route = RouteDescriptor {
            method: RouteMethod::Get,
            path: "/api/projects/{name}/artifacts/{id}",
            transport: RouteTransport::Json,
            auth: RouteAuth::Protected,
            params: Some("api_types::ProjectArtifactParams"),
            query: None,
            body: None,
            response: None,
            event: None,
        };
        assert_eq!(
            render_path(&route, &[("name", "two words".into()), ("id", "7".into())]),
            "/api/projects/two%20words/artifacts/7"
        );
    }
}
