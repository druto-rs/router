//! Route handler contract, thread-safe boxed handlers, and closure adapters.

use std::{future::Future, pin::Pin, sync::Arc};

use crate::{context::HttpContext, error::RouterError, into_response::IntoResponse};

/// Pinned, heap-allocated thread-safe future.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Contract for terminal request handlers.
pub trait Handler: Send + Sync + 'static {
    /// Executes the handler logic against the mutable execution context.
    fn call<'a>(&'a self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
}

/// A reference-counted, thread-safe boxed handler trait object.
pub type BoxedHandler = Arc<dyn Handler>;

impl<F> Handler for F
where
    F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>
        + Send
        + Sync
        + 'static,
{
    fn call<'a>(&'a self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        self(ctx)
    }
}

impl<H: Handler + ?Sized> Handler for Arc<H> {
    fn call<'a>(&'a self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        (**self).call(ctx)
    }
}

/// Adapts an asynchronous closure returning any type implementing [`IntoResponse`] into a [`Handler`].
pub fn from_fn<F, R>(function: F) -> BoxedHandler
where
    F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, R> + Send + Sync + 'static,
    R: IntoResponse + 'static,
{
    struct ClosureHandler<F>(F);

    impl<F, R> Handler for ClosureHandler<F>
    where
        F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, R> + Send + Sync + 'static,
        R: IntoResponse + 'static,
    {
        fn call<'a>(&'a self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
            Box::pin(async move {
                let handler_fn = &self.0;
                let response_value = handler_fn(ctx).await;
                response_value.apply(ctx)
            })
        }
    }

    Arc::new(ClosureHandler(function))
}

/// Helper function to construct a terminal handler closure with explicit higher-ranked type inference.
pub fn from_handler<F>(handler: F) -> F
where
    F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>
        + Send
        + Sync
        + 'static,
{
    handler
}
