# `router` Crate Context

## Purpose

`router` is the HTTP routing, request/response, and execution context layer for the Druto web framework. It bridges
`poth` (radix trie route matching), `dhap` (middleware execution), `cookie` (cookie jars), `khonj` (nested query
parameter parsing), `gopon` (signed URLs), and `session` (session state) into a cohesive, developer-friendly, and
high-performance HTTP layer.

## Reference Architecture

This crate synthesizes the proven architectural patterns of leading web frameworks and Rust systems:

- **AdonisJS** (`@adonisjs/http-server`):
    - Fluent routing API: `Router`, `Route`, `RouteGroup`, `RouteResource`, `BriskRoute`.
    - Scoped hierarchy: path prefixes, named route prefixes, group middleware inheritance, parameter matchers.
    - Core HTTP primitives: `Request`, `Response`, `HttpContext`, `Redirect`, `UrlBuilder`, `SignedUrlBuilder`.
    - Unified `exception_handler` and HTTP method spoofing (`_method` & `X-HTTP-Method-Override`).
- **Laravel** (`illuminate/routing` & `illuminate/http`):
    - Declarative route registration, route naming, and URL generation.
    - Resource controllers with 7 standard RESTful actions (`index`, `create`, `store`, `show`, `edit`, `update`,
      `destroy`).
    - Route groups, fallback catch-all routes (`Route::fallback`), method spoofing (`_method`).
    - Middleware onion runner via pipeline.
- **Cot** (`cot::router` & `cot::request`):
    - Radix trie routing with deterministic route tree compilation and conflict resolution.
    - Reverse routing and typed parameter reconstruction.
- **Topcoat** (`topcoat-router`):
    - Clean separation of builder phase (`RouterBuilder`) from compiled routing table (`Router`).
    - Pre-allocated method slots and standard method dispatch.
    - Type-safe request extensions (`Extensions`) container.
- **Loco.rs** (`loco_rs::controller`):
    - High-level controller routing definitions, prefix normalization, and request context exposure.

## Architectural Lifecycle: Two-Phase Router

Following the pattern in AdonisJS, Topcoat, and Cot, routing operates in two distinct phases:

1. **Definition / Builder Phase (`Router`)**:
    - Mutable registration of routes, nested groups, resource controllers, and brisk routes.
    - Fluent attachment of middleware, route names, parameter constraints, and domains.
    - Routes can be inspected, filtered, or manipulated before server boot.
2. **Execution / Dispatch Phase (`CompiledRouter`)**:
    - Compiling the router (`router.compile()`) freezes all route definitions into an immutable, thread-safe
      (`Send + Sync + 'static`) dispatch engine.
    - Route patterns are compiled into `poth::MethodMatcher`.
    - Group and route middleware stacks are flattened and pre-composed with `dhap::Pipeline` per route (`Global` ➔
      `Group (outer to inner)` ➔ `Route` ➔ `Terminal Handler`), eliminating dynamic hierarchy
      traversals and per-request allocations during request handling.
    - Fast, zero-allocation path parameter lookups directly borrowing slices from the request URI.

## Core Responsibilities & Agreed Architecture

1. **Unified Handler Contract & `IntoResponse`**:
    - Route handlers adhere to the unified signature:
      ```rust
      async fn(ctx: &mut HttpContext) -> impl IntoResponse
      ```
    - The `IntoResponse` trait defines:
      ```rust
      pub trait IntoResponse: Send + 'static {
          fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError>;
      }
      ```
    - Built-in `IntoResponse` implementations for `()`, `Response`, `&'static str`, `String`, `serde_json::Value`,
      `Redirect`, and `Result<T, E>`.
    - Applying `IntoResponse` merges status, headers, and body into the pre-existing `ctx.response` without clobbering
      headers, cookies, or session deltas staged by upstream onion middleware. Handlers can also mutate `ctx.response`
      directly and return `Ok(())`.

2. **Request Body Representation & Non-Destructive Parsing**:
    - Incoming request bodies are buffered in memory as `Bytes` by default (bounded by `max_body_size`, e.g. 2MB,
      returning `413 Payload Too Large` when exceeded).
    - Enables non-destructive, repeated inspection and deserialization across multiple middleware layers and terminal
      handlers: `request.json::<T>()`, `request.form::<T>()`, `request.text()`, and `request.bytes()`.
    - Routes handling large uploads can explicitly opt into raw streaming bodies via route configuration.

3. **Quad-Surface Route Expression Architecture**:
   Druto offers four complementary, interoperable paradigms for declaring routes, all compiling down to the core
   `Router` builder engine:
    - **Fluent Builder API**: `router.get(...)`, `router.post(...)`, `router.group(|g| { ... })`,
      `router.resource(...)`.
    - **Modular Sub-Router Nesting**: `router.nest("/users", users::router())`, allowing modules and crates to build,
      test, and encapsulate their own isolated routers before composing them into the parent app.
    - **Declarative Manifest Macro**: `routes! { ... }` providing a compile-time, bird's-eye view DSL of routes, groups,
      resources, and fallbacks.
    - **Attribute & Controller Proc-Macros**: `#[get("/...")]`, `#[post("/...")]`, and `#[controller]` (via companion
      macro crate) for co-locating route annotations directly on handler functions and controller structs.

4. **Fluent Route Declaration & Scoped Groups**:
    - Method helpers: `get`, `post`, `put`, `patch`, `delete`, `head`, `options`, and `any`.
    - Multi-method matching: `router.match_methods(&[Method::Get, Method::Post], path, handler)`.
    - Named routes: `.name("users.show")` or `.as("users.show")` for reverse URL resolution.
    - Route parameter constraints: `.where("id", BuiltinConstraint::Numeric)` or custom regex patterns.
    - Nested route groups (`router.group(|group| { ... })`):
        - Path prefix inheritance (`.prefix("/api/v1")`).
        - Route name prefix inheritance (`.as("api.v1.")`).
        - Middleware stacking: outer group middleware runs before inner group middleware.
    - Brisk routes and shortcuts:
        - `router.redirect(from, to, status_code)` (HTTP 301/302/307/308 redirection).
        - `router.fallback(handler)` (custom catch-all route for unhandled paths / SPA hosting).

5. **Resource Controller Routing (`RouteResource`)**:
    - `ResourceController` trait representing the 7 conventional RESTful actions with default `405 Method Not Allowed`
      fallbacks:
      ```rust
      pub trait ResourceController: Send + Sync + 'static {
          fn index<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
          fn store<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
          fn show<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
          fn update<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
          fn destroy<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
          fn create<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
          fn edit<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>>;
      }
      ```
    - Fluent modifiers: `router.resource("posts", PostsController).api_only()`, `.only(&[...])`, `.except(&[...])`.
    - Automated nested resource parameter naming (e.g. `posts.comments` ➔ `:post_id`, `:id`).

6. **Type-Safe Request Extensions (`ctx.extensions`)**:
    - `HttpContext` contains a type-safe `Extensions` container (type map indexed by `TypeId`).
    - Supports `insert<T>`, `get<T>`, `get_mut<T>`, and `remove<T>`, allowing middleware (such as `crates/auth`,
      tenancy, or telemetry) to pass arbitrary typed data down to route handlers with complete compile-time type safety.

7. **HTTP Method Spoofing**:
    - Configurable via `RouterConfig.allow_method_spoofing` (enabled by default).
    - On incoming `POST` requests, checks `_method` in form body/query parameters or `X-HTTP-Method-Override` headers.
    - Exposes `request.intended_method()` for the raw transport verb (`POST`) and `request.method()` for the effective
      verb (`PUT`, `PATCH`, `DELETE`), which the router matches against.

8. **Reverse Routing & Cryptographic Signed URLs (`UrlGenerator`)**:
    - `UrlGenerator` combines `poth::MethodMatcher::build_path` with `gopon::UrlSigner`.
    - Exposed on `CompiledRouter` and `HttpContext`: `ctx.url_for("users.show", &[("id", "42")])` and
      `ctx.signed_url_for("verify.email", &[("id", "42")], ttl)`.
    - `Request` provides verification helpers: `request.has_valid_signature(&signer)` and
      `request.has_valid_signature_until(&signer)`.

9. **Automatic RFC 9110 Protocol Compliance**:
    - Automatic `OPTIONS`: If no explicit `OPTIONS` route is defined, automatically returns `204 No Content` with an
      `Allow` header containing all supported verbs for that path (RFC 9110 §9.3.7).
    - Automatic `HEAD`: Falls back to the matching `GET` handler and strips the response body (RFC 9110 §9.3.2).
    - Automatic `405 Method Not Allowed`: Returns `405` with `Allow` header when a path matches other verbs.
    - `router.fallback(handler)`: Executes custom fallback handler for unrouted paths before falling back to default
      `404`.

10. **Error Taxonomy & Automatic HTTP Response Mapping**:

- `enum RouterError` implements `IntoResponse`, auto-mapping errors to RFC-compliant status codes:
    - `NotFound` ➔ `404 Not Found`
    - `MethodNotAllowed { allowed }` ➔ `405 Method Not Allowed` with `Allow` header
    - `PayloadTooLarge { max_bytes, received_bytes }` ➔ `413 Payload Too Large`
    - `InvalidSignature` ➔ `403 Forbidden`
    - `InvalidParams { message }` ➔ `400 Bad Request`
    - `DeserializationError(String)` ➔ `422 Unprocessable Entity`
    - `Internal(String)` ➔ `500 Internal Server Error`
- Custom global error formatting overridable via `router.error_handler(...)`.

## Boundaries & Non-Responsibilities

- **Transport Agnostic**: Does not open TCP listeners or depend directly on Hyper/Actix-net socket loops. It takes
  abstract requests and outputs abstract responses.
- **Not Authentication**: Authentication guards and tokens belong in `crates/auth`.
- **Not Database/ORM**: Database queries belong in `crates/db`. Handlers interface with models and repositories.

## Implemented Public Surface

- `struct Router`: Mutable builder for registering routes, groups, resources, and configuration.
- `struct CompiledRouter`: Immutable, thread-safe pre-compiled dispatch engine.
- `struct Route`: Route definition with method, pattern, handler, name, constraints, and middleware.
- `struct RouteGroup`: Scoped route container inheriting prefixes, names, and middleware.
- `struct RouteResource`: RESTful resource controller builder supporting `.only()`, `.except()`, `.api_only()`.
- `trait ResourceController`: Contract for 7-action REST controllers.
- `trait IntoResponse`: Conversion and merging into `ctx.response`.
- `struct Request` & `struct RequestBuilder`: Incoming HTTP request representation with content negotiation, proxy
  evaluation, and signed URL verification.
- `struct Response`: Outgoing HTTP response representation.
- `struct HttpContext`: Request execution context passed to handlers and middleware, implementing
  `session::SessionContext`.
- `struct Extensions`: Type-safe request-local storage bag.
- `struct UrlGenerator`: Reverse route and signed URL generator.
- `struct StatusCode`: RFC 9110 status codes with canonical reason phrases and classifications.
- `struct HeaderName`, `struct HeaderValue`, `struct HeaderMap`: Case-insensitive HTTP header container.
- `struct Redirect`: Fluent redirection response builder.
- `enum ResponseBody`: Encapsulated response body (`Empty`, `Bytes`, `Text`, `Html`, `Json`).
- `enum RouterError`: Strongly typed routing and dispatch errors implementing `IntoResponse`.
- `macro routes!`: Declarative DSL macro for bird's-eye route, group, resource, and nesting definitions.
- `fn from_fn`, `fn from_handler`: Type-erased handler constructors for asynchronous functions and closures.
