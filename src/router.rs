//! Main route registry, group scoping, and two-phase compilation builder.

use std::sync::Arc;

use dhap::{BoxedMiddleware, Middleware};
use gopon::UrlSigner;
use poth::Method;

use crate::{
    compiled::CompiledRouter,
    context::HttpContext,
    error::RouterError,
    handler::{BoxFuture, BoxedHandler, Handler, from_fn},
    into_response::IntoResponse,
    resource::{ResourceController, RouteResource},
    response::Response,
    route::Route,
    status::StatusCode,
};

/// Global configuration options for the router and request parsing.
#[derive(Clone, Debug)]
pub struct RouterConfig {
    /// Maximum allowed request body size in bytes before returning 413 Payload Too Large.
    pub max_body_size: usize,
    /// Whether to evaluate `_method` and `X-HTTP-Method-Override` on POST requests.
    pub allow_method_spoofing: bool,
    /// Whether to honor `Forwarded` and `X-Forwarded-*` proxy headers when resolving IP and scheme.
    pub trust_proxy: bool,
    /// Cryptographic URL signer used for generating and verifying signed URLs.
    pub url_signer: Option<UrlSigner>,
    /// Optional base URL prefix (e.g. `https://example.com`) for reverse URL generation.
    pub base_url: Option<String>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            max_body_size: 2 * 1024 * 1024, // 2MB default ceiling
            allow_method_spoofing: true,
            trust_proxy: false,
            url_signer: None,
            base_url: None,
        }
    }
}

/// Custom global error handler signature.
pub type ErrorHandlerFn = Arc<
    dyn Fn(&mut HttpContext, RouterError) -> BoxFuture<'_, Result<(), RouterError>>
        + Send
        + Sync
        + 'static,
>;

/// Mutable builder for declaring routes, nested groups, and resource controllers.
pub struct Router {
    global_middlewares: Vec<BoxedMiddleware<HttpContext, Response, RouterError>>,
    routes: Vec<Route>,
    fallback_handler: Option<BoxedHandler>,
    error_handler: Option<ErrorHandlerFn>,
    config: RouterConfig,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    /// Creates a new empty `Router` with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            global_middlewares: Vec::new(),
            routes: Vec::new(),
            fallback_handler: None,
            error_handler: None,
            config: RouterConfig::default(),
        }
    }

    /// Creates a new `Router` with custom configuration.
    #[must_use]
    pub fn with_config(config: RouterConfig) -> Self {
        Self {
            global_middlewares: Vec::new(),
            routes: Vec::new(),
            fallback_handler: None,
            error_handler: None,
            config,
        }
    }

    /// Returns a mutable reference to the router configuration.
    pub fn config_mut(&mut self) -> &mut RouterConfig {
        &mut self.config
    }

    /// Appends a global middleware that executes for all incoming requests.
    pub fn middleware<M>(&mut self, middleware: M) -> &mut Self
    where
        M: Middleware<HttpContext, Response, RouterError>,
    {
        self.global_middlewares.push(Arc::new(middleware));
        self
    }

    /// Appends an already boxed thread-safe global middleware.
    pub fn middleware_arc(
        &mut self,
        middleware: BoxedMiddleware<HttpContext, Response, RouterError>,
    ) -> &mut Self {
        self.global_middlewares.push(middleware);
        self
    }

    /// Registers a route responding to the specified HTTP methods.
    pub fn match_methods(
        &mut self,
        methods: &[Method],
        pattern: impl Into<String>,
        handler: impl Handler,
    ) -> &mut Route {
        let index = self.routes.len();
        self.routes
            .push(Route::new(methods.to_vec(), pattern, Arc::new(handler)));
        &mut self.routes[index]
    }

    /// Registers a GET route.
    pub fn get(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Get], pattern, handler)
    }

    /// Registers a POST route.
    pub fn post(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Post], pattern, handler)
    }

    /// Registers a PUT route.
    pub fn put(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Put], pattern, handler)
    }

    /// Registers a PATCH route.
    pub fn patch(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Patch], pattern, handler)
    }

    /// Registers a DELETE route.
    pub fn delete(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Delete], pattern, handler)
    }

    /// Registers a HEAD route.
    pub fn head(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Head], pattern, handler)
    }

    /// Registers an OPTIONS route.
    pub fn options(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Options], pattern, handler)
    }

    /// Registers a route that matches any HTTP verb.
    pub fn any(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Any], pattern, handler)
    }

    /// Registers a GET route from an asynchronous closure.
    pub fn get_fn<F, R>(&mut self, pattern: impl Into<String>, function: F) -> &mut Route
    where
        F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, R> + Send + Sync + 'static,
        R: IntoResponse + 'static,
    {
        self.match_methods(&[Method::Get], pattern, from_fn(function))
    }

    /// Registers a POST route from an asynchronous closure.
    pub fn post_fn<F, R>(&mut self, pattern: impl Into<String>, function: F) -> &mut Route
    where
        F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, R> + Send + Sync + 'static,
        R: IntoResponse + 'static,
    {
        self.match_methods(&[Method::Post], pattern, from_fn(function))
    }

    /// Registers a scoped route group through a closure.
    pub fn group<F>(&mut self, configure: F) -> &mut Self
    where
        F: FnOnce(&mut crate::group::RouteGroup),
    {
        let mut group = crate::group::RouteGroup::new();
        configure(&mut group);
        self.routes.extend(group.into_routes());
        self
    }

    /// Registers a scoped route group with an initial path prefix.
    pub fn group_prefixed<F>(&mut self, prefix: impl Into<String>, configure: F) -> &mut Self
    where
        F: FnOnce(&mut crate::group::RouteGroup),
    {
        let mut group = crate::group::RouteGroup::new();
        group.prefix(prefix);
        configure(&mut group);
        self.routes.extend(group.into_routes());
        self
    }

    /// Registers a RESTful resource controller.
    pub fn resource(&mut self, name: impl Into<String>, controller: impl ResourceController) {
        let mut resource_routes = RouteResource::new(name, Arc::new(controller)).into_routes();
        self.routes.append(&mut resource_routes);
    }

    /// Registers a RESTful resource controller with custom configuration.
    pub fn resource_with<F>(
        &mut self,
        name: impl Into<String>,
        controller: impl ResourceController,
        configure: F,
    ) -> &mut Self
    where
        F: FnOnce(&mut RouteResource),
    {
        let mut resource = RouteResource::new(name, Arc::new(controller));
        configure(&mut resource);
        self.routes.extend(resource.into_routes());
        self
    }

    /// Nests an independent sub-router under the given path prefix.
    pub fn nest(&mut self, prefix: impl Into<String>, sub_router: Self) -> &mut Self {
        let prefix_str = prefix.into();
        for mut route in sub_router.routes {
            route.prefix_pattern(&prefix_str);
            self.routes.push(route);
        }
        self
    }

    /// Nests an independent sub-router with both a path prefix and a route name namespace.
    pub fn nest_as(
        &mut self,
        prefix: impl Into<String>,
        name_prefix: impl Into<String>,
        sub_router: Self,
    ) -> &mut Self {
        let prefix_str = prefix.into();
        let name_str = name_prefix.into();
        for mut route in sub_router.routes {
            route.prefix_pattern(&prefix_str);
            route.prefix_name(&name_str);
            self.routes.push(route);
        }
        self
    }

    /// Convenience helper to register an HTTP redirect route.
    pub fn redirect(
        &mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        status: StatusCode,
    ) -> &mut Route {
        let destination = to.into();
        let handler = from_fn(move |ctx: &mut HttpContext| {
            let dest = destination.clone();
            Box::pin(async move {
                ctx.response.redirect(dest, Some(status));
                Ok::<(), RouterError>(())
            })
        });
        self.get(from, handler)
    }

    /// Registers a custom fallback catch-all handler for unrouted paths.
    pub fn fallback(&mut self, handler: impl Handler) -> &mut Self {
        self.fallback_handler = Some(Arc::new(handler));
        self
    }

    /// Registers a custom global error formatting handler.
    pub fn error_handler<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&mut HttpContext, RouterError) -> BoxFuture<'_, Result<(), RouterError>>
            + Send
            + Sync
            + 'static,
    {
        self.error_handler = Some(Arc::new(handler));
        self
    }

    /// Compiles and freezes all registered routes into an immutable, thread-safe [`CompiledRouter`].
    pub fn compile(self) -> Result<CompiledRouter, RouterError> {
        CompiledRouter::compile(
            self.routes,
            self.global_middlewares,
            self.fallback_handler,
            self.error_handler,
            self.config,
        )
    }
}
