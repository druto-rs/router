//! RESTful resource routing, controller contracts, and automatic conventional action expansion.

use std::sync::Arc;

use dhap::{BoxedMiddleware, Middleware};
use poth::Method;

use crate::{
    context::HttpContext,
    error::RouterError,
    handler::{BoxFuture, BoxedHandler, Handler},
    response::Response,
    route::Route,
};

/// The conventional RESTful actions supported by a resource controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceAction {
    /// List all resources (`GET /resource`).
    Index,
    /// Display HTML form to create a new resource (`GET /resource/create`).
    Create,
    /// Persist a newly created resource (`POST /resource`).
    Store,
    /// Display a specific resource by identifier (`GET /resource/:id`).
    Show,
    /// Display HTML form to edit an existing resource (`GET /resource/:id/edit`).
    Edit,
    /// Update an existing resource (`PUT` / `PATCH /resource/:id`).
    Update,
    /// Delete an existing resource (`DELETE /resource/:id`).
    Destroy,
}

impl ResourceAction {
    /// Returns the action name as a lowercase string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Create => "create",
            Self::Store => "store",
            Self::Show => "show",
            Self::Edit => "edit",
            Self::Update => "update",
            Self::Destroy => "destroy",
        }
    }
}

/// Trait defining the conventional 7-action RESTful controller contract.
///
/// Unimplemented actions default to returning a [`RouterError::MethodNotAllowed`].
pub trait ResourceController: Send + Sync + 'static {
    /// Handles `GET /resources`.
    fn index<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let _ = ctx;
        Box::pin(async move {
            Err(RouterError::MethodNotAllowed {
                allowed: Vec::new(),
            })
        })
    }

    /// Handles `GET /resources/create`.
    fn create<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let _ = ctx;
        Box::pin(async move {
            Err(RouterError::MethodNotAllowed {
                allowed: Vec::new(),
            })
        })
    }

    /// Handles `POST /resources`.
    fn store<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let _ = ctx;
        Box::pin(async move {
            Err(RouterError::MethodNotAllowed {
                allowed: Vec::new(),
            })
        })
    }

    /// Handles `GET /resources/:id`.
    fn show<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let _ = ctx;
        Box::pin(async move {
            Err(RouterError::MethodNotAllowed {
                allowed: Vec::new(),
            })
        })
    }

    /// Handles `GET /resources/:id/edit`.
    fn edit<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let _ = ctx;
        Box::pin(async move {
            Err(RouterError::MethodNotAllowed {
                allowed: Vec::new(),
            })
        })
    }

    /// Handles `PUT` / `PATCH /resources/:id`.
    fn update<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let _ = ctx;
        Box::pin(async move {
            Err(RouterError::MethodNotAllowed {
                allowed: Vec::new(),
            })
        })
    }

    /// Handles `DELETE /resources/:id`.
    fn destroy<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let _ = ctx;
        Box::pin(async move {
            Err(RouterError::MethodNotAllowed {
                allowed: Vec::new(),
            })
        })
    }
}

struct ResourceActionHandler {
    controller: Arc<dyn ResourceController>,
    action: ResourceAction,
}

impl Handler for ResourceActionHandler {
    fn call<'a>(&'a self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        match self.action {
            ResourceAction::Index => self.controller.index(ctx),
            ResourceAction::Create => self.controller.create(ctx),
            ResourceAction::Store => self.controller.store(ctx),
            ResourceAction::Show => self.controller.show(ctx),
            ResourceAction::Edit => self.controller.edit(ctx),
            ResourceAction::Update => self.controller.update(ctx),
            ResourceAction::Destroy => self.controller.destroy(ctx),
        }
    }
}

/// Fluent builder for configuring and expanding a RESTful resource route set.
pub struct RouteResource {
    name: String,
    controller: Arc<dyn ResourceController>,
    actions: Vec<ResourceAction>,
    custom_name: Option<String>,
    middlewares: Vec<BoxedMiddleware<HttpContext, Response, RouterError>>,
}

impl RouteResource {
    /// Creates a new `RouteResource` registering all 7 actions for the given resource path.
    #[must_use]
    pub fn new(name: impl Into<String>, controller: Arc<dyn ResourceController>) -> Self {
        Self {
            name: name.into(),
            controller,
            actions: vec![
                ResourceAction::Index,
                ResourceAction::Create,
                ResourceAction::Store,
                ResourceAction::Show,
                ResourceAction::Edit,
                ResourceAction::Update,
                ResourceAction::Destroy,
            ],
            custom_name: None,
            middlewares: Vec::new(),
        }
    }

    /// Restricts the resource to API actions only, removing `create` and `edit` HTML form routes.
    pub fn api_only(&mut self) -> &mut Self {
        self.actions
            .retain(|action| !matches!(action, ResourceAction::Create | ResourceAction::Edit));
        self
    }

    /// Restricts the resource to only the specified actions.
    pub fn only(&mut self, actions: &[ResourceAction]) -> &mut Self {
        self.actions.retain(|action| actions.contains(action));
        self
    }

    /// Excludes the specified actions from the resource.
    pub fn except(&mut self, actions: &[ResourceAction]) -> &mut Self {
        self.actions.retain(|action| !actions.contains(action));
        self
    }

    /// Overrides the base route name prefix for reverse URL generation.
    pub fn r#as(&mut self, name: impl Into<String>) -> &mut Self {
        self.custom_name = Some(name.into());
        self
    }

    /// Alias for `.as(name)` to assign a custom route name prefix.
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        self.r#as(name)
    }

    /// Returns the active actions configured for this resource.
    #[must_use]
    pub fn actions(&self) -> &[ResourceAction] {
        &self.actions
    }

    /// Appends an already boxed thread-safe middleware layer to all routes generated by this resource.
    pub fn middleware_arc(
        &mut self,
        middleware: BoxedMiddleware<HttpContext, Response, RouterError>,
    ) -> &mut Self {
        self.middlewares.push(middleware);
        self
    }

    /// Appends a middleware layer to all routes generated by this resource.
    pub fn middleware<M>(&mut self, middleware: M) -> &mut Self
    where
        M: Middleware<HttpContext, Response, RouterError>,
    {
        self.middleware_arc(Arc::new(middleware))
    }

    /// Expands the resource configuration into a collection of concrete [`Route`] instances.
    #[must_use]
    pub fn into_routes(self) -> Vec<Route> {
        let (base_path, id_param) = Self::compute_paths(&self.name);
        let base_name = self
            .custom_name
            .unwrap_or_else(|| self.name.replace('/', "."));

        let mut routes = Vec::new();

        for action in self.actions {
            let (methods, pattern, name) = match action {
                ResourceAction::Index => (
                    vec![Method::Get],
                    base_path.clone(),
                    format!("{base_name}.index"),
                ),
                ResourceAction::Create => (
                    vec![Method::Get],
                    format!("{base_path}/create"),
                    format!("{base_name}.create"),
                ),
                ResourceAction::Store => (
                    vec![Method::Post],
                    base_path.clone(),
                    format!("{base_name}.store"),
                ),
                ResourceAction::Show => (
                    vec![Method::Get],
                    format!("{base_path}/:{id_param}"),
                    format!("{base_name}.show"),
                ),
                ResourceAction::Edit => (
                    vec![Method::Get],
                    format!("{base_path}/:{id_param}/edit"),
                    format!("{base_name}.edit"),
                ),
                ResourceAction::Update => (
                    vec![Method::Put, Method::Patch],
                    format!("{base_path}/:{id_param}"),
                    format!("{base_name}.update"),
                ),
                ResourceAction::Destroy => (
                    vec![Method::Delete],
                    format!("{base_path}/:{id_param}"),
                    format!("{base_name}.destroy"),
                ),
            };

            let handler: BoxedHandler = Arc::new(ResourceActionHandler {
                controller: Arc::clone(&self.controller),
                action,
            });

            let mut route = Route::new(methods, pattern, handler);
            route.r#as(name);
            for middleware in &self.middlewares {
                route.middleware_arc(Arc::clone(middleware));
            }
            routes.push(route);
        }

        routes
    }

    fn compute_paths(resource_name: &str) -> (String, String) {
        let normalized = resource_name.trim_matches('/');
        let segments: Vec<&str> = if normalized.contains('.') {
            normalized.split('.').collect()
        } else {
            normalized.split('/').collect()
        };

        if segments.len() <= 1 {
            let base = format!("/{}", segments.first().copied().unwrap_or(""));
            return (base, "id".to_string());
        }

        let mut path_parts = Vec::new();
        let last_idx = segments.len() - 1;

        for (idx, &segment) in segments.iter().enumerate() {
            path_parts.push(segment.to_string());
            if idx < last_idx {
                let singular = segment.strip_suffix('s').unwrap_or(segment);
                path_parts.push(format!(":{singular}_id"));
            }
        }

        (format!("/{}", path_parts.join("/")), "id".to_string())
    }
}
