//! Single route declaration, HTTP method binding, parameter constraints, and route middleware.

use std::{collections::HashMap, sync::Arc};

use dhap::{BoxedMiddleware, Middleware};
use poth::{Constraint, Method};

use crate::{context::HttpContext, error::RouterError, handler::BoxedHandler, response::Response};

/// A single declared HTTP route mapping methods and a URI pattern to a terminal handler.
#[derive(Clone)]
pub struct Route {
    methods: Vec<Method>,
    pattern: String,
    handler: BoxedHandler,
    name: Option<String>,
    middlewares: Vec<BoxedMiddleware<HttpContext, Response, RouterError>>,
    constraints: HashMap<String, Constraint>,
}

impl Route {
    /// Creates a new route definition.
    #[must_use]
    pub fn new(methods: Vec<Method>, pattern: impl Into<String>, handler: BoxedHandler) -> Self {
        Self {
            methods,
            pattern: pattern.into(),
            handler,
            name: None,
            middlewares: Vec::new(),
            constraints: HashMap::new(),
        }
    }

    /// Sets the route name for reverse URL generation.
    pub fn r#as(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    /// Alias for `.as(name)` to assign a named identifier to the route.
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        self.r#as(name)
    }

    /// Appends a middleware layer to this specific route.
    pub fn middleware<M>(&mut self, middleware: M) -> &mut Self
    where
        M: Middleware<HttpContext, Response, RouterError>,
    {
        self.middleware_arc(Arc::new(middleware))
    }

    /// Appends an already boxed thread-safe middleware layer to this route.
    pub fn middleware_arc(
        &mut self,
        middleware: BoxedMiddleware<HttpContext, Response, RouterError>,
    ) -> &mut Self {
        self.middlewares.push(middleware);
        self
    }

    /// Attaches a validation constraint to a named dynamic path parameter.
    pub fn r#where(&mut self, param: &str, constraint: Constraint) -> &mut Self {
        self.constraints.insert(param.to_string(), constraint);
        self
    }

    /// Attaches an ASCII numeric constraint (`^[0-9]+$`) to a named dynamic path parameter.
    pub fn where_num(&mut self, param: &str) -> &mut Self {
        self.r#where(param, Constraint::num())
    }

    /// Attaches a UUID constraint to a named dynamic path parameter.
    pub fn where_uuid(&mut self, param: &str) -> &mut Self {
        self.r#where(param, Constraint::uuid())
    }

    /// Attaches an alphabetic constraint (`^[a-zA-Z]+$`) to a named dynamic path parameter.
    pub fn where_alpha(&mut self, param: &str) -> &mut Self {
        self.r#where(param, Constraint::alpha())
    }

    /// Attaches an alphanumeric constraint (`^[a-zA-Z0-9]+$`) to a named dynamic path parameter.
    pub fn where_alphanumeric(&mut self, param: &str) -> &mut Self {
        self.r#where(param, Constraint::alphanumeric())
    }

    /// Attaches a URL slug constraint to a named dynamic path parameter.
    pub fn where_slug(&mut self, param: &str) -> &mut Self {
        self.r#where(param, Constraint::slug())
    }

    /// Attaches a regex validation constraint to a named dynamic path parameter.
    pub fn where_regex(&mut self, param: &str, pattern: &str) -> Result<&mut Self, regex::Error> {
        let regex = regex::Regex::new(pattern)?;
        self.constraints.insert(
            param.to_string(),
            Constraint::Regex {
                pattern: regex,
                source: pattern.to_string(),
            },
        );
        Ok(self)
    }

    /// Returns the HTTP methods this route responds to.
    #[must_use]
    pub fn methods(&self) -> &[Method] {
        &self.methods
    }

    /// Returns the URI pattern string.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Returns a reference to the boxed route handler.
    #[must_use]
    pub fn handler(&self) -> &BoxedHandler {
        &self.handler
    }

    /// Returns the route name if assigned.
    #[must_use]
    pub fn route_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the middleware layers attached to this route.
    #[must_use]
    pub fn middlewares(&self) -> &[BoxedMiddleware<HttpContext, Response, RouterError>] {
        &self.middlewares
    }

    /// Returns the parameter constraints attached to this route.
    #[must_use]
    pub fn constraints(&self) -> &HashMap<String, Constraint> {
        &self.constraints
    }

    /// Prepends a path prefix to the route pattern.
    pub fn prefix_pattern(&mut self, prefix: &str) {
        let trimmed_prefix = prefix.trim_end_matches('/');
        let trimmed_pattern = self.pattern.trim_start_matches('/');

        if trimmed_prefix.is_empty() {
            if !trimmed_pattern.starts_with('/') {
                self.pattern = format!("/{trimmed_pattern}");
            }
        } else {
            self.pattern = format!("{trimmed_prefix}/{trimmed_pattern}");
        }
    }

    /// Prepends a namespace prefix to the route name if defined.
    pub fn prefix_name(&mut self, prefix: &str) {
        if let Some(existing_name) = &self.name {
            self.name = Some(format!("{prefix}{existing_name}"));
        }
    }

    /// Prepends a middleware layer (used when inheriting group middleware).
    pub fn prepend_middleware(
        &mut self,
        middleware: BoxedMiddleware<HttpContext, Response, RouterError>,
    ) {
        self.middlewares.insert(0, middleware);
    }
}
