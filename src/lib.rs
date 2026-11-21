//! HTTP routing, request/response lifecycle, middleware onion runner, and execution context.
//!
//! # Architecture Overview
//!
//! - **Two-Phase Routing Engine**: Route registration via mutable builder [`Router`], followed by
//!   compilation into a frozen, thread-safe [`CompiledRouter`].
//! - **Zero-Allocation Dispatch**: Flattened middleware stacks pre-compiled into [`dhap::Pipeline`] instances
//!   stored inside [`poth::MethodMatcher`].
//! - **Unified Context & Pipeline Onion**: Handlers and middleware operate over [`HttpContext`], integrating
//!   cookies, sessions, request-scoped extensions, and parameters.
//! - **RESTful Resource Controllers**: Conventional 7-action routing with [`ResourceController`] and [`RouteResource`].
//! - **Multi-Surface Routing**: Fluent builder API, modular sub-router nesting ([`Router::nest`]), and declarative
//!   manifest macros ([`routes!`]).
//! - **Automated RFC 9110 Compliance**: Automatic `OPTIONS` and `HEAD` execution with status `405 Method Not Allowed`
//!   and `Allow` headers.
//! - **Reverse Routing & Signed URLs**: Named route path synthesis and HMAC tamper-proof links via [`UrlGenerator`].

pub mod compiled;
pub mod context;
pub mod error;
pub mod extensions;
pub mod group;
pub mod handler;
pub mod header;
pub mod into_response;
pub mod macros;
pub mod request;
pub mod resource;
pub mod response;
pub mod route;
pub mod router;
pub mod status;
pub mod url_generator;
pub mod validator;

pub use compiled::CompiledRouter;
pub use context::HttpContext;
pub use error::RouterError;
pub use extensions::Extensions;
pub use group::RouteGroup;
pub use handler::{BoxFuture, BoxedHandler, Handler, from_fn, from_handler};
pub use header::{HeaderMap, HeaderName, HeaderValue};
pub use into_response::IntoResponse;
pub use poth::{BuiltinConstraint, Constraint, Method};
pub use request::{Request, RequestBuilder, RouteParams};
pub use resource::{ResourceAction, ResourceController, RouteResource};
pub use response::{Redirect, Response, ResponseBody};
pub use route::Route;
pub use router::{ErrorHandlerFn, Router, RouterConfig};
pub use status::StatusCode;
pub use url_generator::UrlGenerator;
pub use validator::HttpContextValidateExt;

/// Common imports and traits for building HTTP applications and handlers.
pub mod prelude {
    pub use crate::{
        CompiledRouter, ErrorHandlerFn, HeaderMap, HeaderName, HeaderValue, HttpContext,
        HttpContextValidateExt, IntoResponse, Method, Redirect, Request, RequestBuilder,
        ResourceAction, ResourceController, Response, ResponseBody, Route, RouteGroup,
        RouteResource, Router, RouterConfig, RouterError, StatusCode, UrlGenerator, from_fn,
        from_handler, routes,
    };
}
