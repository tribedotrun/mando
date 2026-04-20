use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl RouteMethod {
    pub const fn as_http(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteTransport {
    Json,
    Multipart,
    Sse,
    Static,
    Ndjson,
}

impl RouteTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Multipart => "multipart",
            Self::Sse => "sse",
            Self::Static => "static",
            Self::Ndjson => "ndjson",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteAuth {
    Public,
    Protected,
}

impl RouteAuth {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Protected => "protected",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RouteRegistration {
    pub method: RouteMethod,
    pub path: &'static str,
    pub transport: RouteTransport,
    pub auth: RouteAuth,
    pub body_ty: Option<&'static str>,
    pub query_ty: Option<&'static str>,
    pub params_ty: Option<&'static str>,
    pub res_ty: Option<&'static str>,
    pub event_ty: Option<&'static str>,
}

inventory::collect!(RouteRegistration);

pub fn route_registrations() -> Vec<RouteRegistration> {
    let mut routes: Vec<_> = inventory::iter::<RouteRegistration>
        .into_iter()
        .copied()
        .collect();
    routes.sort_by(|a, b| {
        a.path
            .cmp(b.path)
            .then_with(|| a.method.as_http().cmp(b.method.as_http()))
    });
    routes
}
