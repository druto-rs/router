//! Scoped route grouping, hierarchical prefixing, and middleware inheritance.

use std::sync::Arc;

use dhap::{BoxedMiddleware, Middleware};
use poth::Method;

use crate::{
    context::HttpContext,
    error::RouterError,
    handler::{BoxFuture, Handler, from_fn},
    into_response::IntoResponse,
    resource::{ResourceController, RouteResource},
    response::Response,
    route::Route,
};

/// A scoped route group providing path prefixing, route naming namespaces, and shared middleware.
pub struct RouteGroup {
    prefix: Option<String>,
    name_prefix: Option<String>,
    middlewares: Vec<BoxedMiddleware<HttpContext, Response, RouterError>>,
    routes: Vec<Route>,
}

impl Default for RouteGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteGroup {
    /// Creates a new empty `RouteGroup`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefix: None,
            name_prefix: None,
            middlewares: Vec::new(),
            routes: Vec::new(),
        }
    }

    /// Sets the path prefix for all routes in this group (e.g. `"/api/v1"`).
    pub fn prefix(&mut self, prefix: impl Into<String>) -> &mut Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Sets the name namespace prefix for all routes in this group (e.g. `"api.v1."`).
    pub fn r#as(&mut self, name_prefix: impl Into<String>) -> &mut Self {
        self.name_prefix = Some(name_prefix.into());
        self
    }

    /// Alias for `.as(name_prefix)`.
    pub fn name(&mut self, name_prefix: impl Into<String>) -> &mut Self {
        self.r#as(name_prefix)
    }

    /// Appends a middleware layer that will execute for all routes in this group.
    pub fn middleware<M>(&mut self, middleware: M) -> &mut Self
    where
        M: Middleware<HttpContext, Response, RouterError>,
    {
        self.middleware_arc(Arc::new(middleware))
    }

    /// Appends an already boxed thread-safe middleware layer to this group.
    pub fn middleware_arc(
        &mut self,
        middleware: BoxedMiddleware<HttpContext, Response, RouterError>,
    ) -> &mut Self {
        self.middlewares.push(middleware);
        self
    }

    /// Registers a route responding to the specified HTTP methods.
    pub fn match_methods(
        &mut self,
        methods: &[Method],
        pattern: impl Into<String>,
        handler: impl Handler,
    ) -> &mut Route {
        let route = Route::new(methods.to_vec(), pattern, Arc::new(handler));
        self.routes.push(route);
        self.routes.last_mut().expect("route was just pushed")
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

    /// Registers a route responding to any HTTP method.
    pub fn any(&mut self, pattern: impl Into<String>, handler: impl Handler) -> &mut Route {
        self.match_methods(&[Method::Any], pattern, handler)
    }

    /// Registers a GET route from an asynchronous closure returning any [`IntoResponse`] type.
    pub fn get_fn<F, R>(&mut self, pattern: impl Into<String>, function: F) -> &mut Route
    where
        F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, R> + Send + Sync + 'static,
        R: IntoResponse + 'static,
    {
        self.get(pattern, from_fn(function))
    }

    /// Registers a POST route from an asynchronous closure returning any [`IntoResponse`] type.
    pub fn post_fn<F, R>(&mut self, pattern: impl Into<String>, function: F) -> &mut Route
    where
        F: for<'a> Fn(&'a mut HttpContext) -> BoxFuture<'a, R> + Send + Sync + 'static,
        R: IntoResponse + 'static,
    {
        self.post(pattern, from_fn(function))
    }

    /// Registers a RESTful resource controller inside this group.
    pub fn resource(&mut self, name: impl Into<String>, controller: impl ResourceController) {
        let resource = RouteResource::new(name, Arc::new(controller));
        self.routes.extend(resource.into_routes());
    }

    /// Registers and fluently configures a RESTful resource controller inside this group.
    pub fn resource_with<F>(
        &mut self,
        name: impl Into<String>,
        controller: impl ResourceController,
        configure: F,
    ) where
        F: FnOnce(&mut RouteResource),
    {
        let mut resource = RouteResource::new(name, Arc::new(controller));
        configure(&mut resource);
        self.routes.extend(resource.into_routes());
    }

    /// Nests a sub-group inside this group.
    pub fn group<F>(&mut self, configure: F)
    where
        F: FnOnce(&mut RouteGroup),
    {
        let mut sub_group = RouteGroup::new();
        configure(&mut sub_group);
        let sub_routes = sub_group.into_routes();
        self.routes.extend(sub_routes);
    }

    /// Nests a sub-group with an initial path prefix inside this group.
    pub fn group_prefixed<F>(&mut self, prefix: impl Into<String>, configure: F)
    where
        F: FnOnce(&mut RouteGroup),
    {
        let mut sub_group = RouteGroup::new();
        sub_group.prefix(prefix);
        configure(&mut sub_group);
        let sub_routes = sub_group.into_routes();
        self.routes.extend(sub_routes);
    }

    /// Appends an existing route into this group.
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
    }

    /// Flattens this group, applying its prefix, name prefix, and middleware to all contained routes.
    #[must_use]
    pub fn into_routes(mut self) -> Vec<Route> {
        for route in &mut self.routes {
            if let Some(ref prefix) = self.prefix {
                route.prefix_pattern(prefix);
            }
            if let Some(ref name_prefix) = self.name_prefix {
                route.prefix_name(name_prefix);
            }
            for middleware in self.middlewares.iter().rev() {
                route.prepend_middleware(Arc::clone(middleware));
            }
        }
        self.routes
    }
}
