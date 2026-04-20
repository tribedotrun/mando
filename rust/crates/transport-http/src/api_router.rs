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
