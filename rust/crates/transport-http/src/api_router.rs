use axum::routing::MethodRouter;
use axum::Router;

pub struct ApiRouter<S = ()> {
    inner: Router<S>,
}

impl<S> ApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            inner: Router::new(),
        }
    }

    pub(crate) fn route(self, path: &'static str, method_router: MethodRouter<S>) -> Self {
        Self {
            inner: self.inner.route(path, method_router),
        }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            inner: self.inner.merge(other.inner),
        }
    }

    /// Override Axum's default 2 MiB extractor limit for this route group.
    pub(crate) fn with_body_limit(self, bytes: usize) -> Self {
        Self {
            inner: self
                .inner
                .layer(axum::extract::DefaultBodyLimit::max(bytes)),
        }
    }

    pub fn into_router(self) -> Router<S> {
        self.inner
    }
}

impl<S> Default for ApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

pub fn contract_inventory_link_anchor() {}
