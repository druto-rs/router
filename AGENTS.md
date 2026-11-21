# `router` Agent Instructions

## Scope

This crate owns the HTTP routing engine, request/response models, middleware onion runner, and execution context
(`HttpContext`).
It bridges `poth`, `dhap`, `cookie`, `khonj`, `gopon`, and `session` without coupling directly to concrete
transport socket listeners.

## Design Rules

1. **Transport & Runtime Agnostic**:
    - The router must not depend directly on concrete transport socket listeners or be tightly coupled to an async
      runtime.
    - It processes abstract HTTP requests (`Request`) and produces abstract HTTP responses (`Response`).
2. **Two-Phase Lifecycle (Builder vs Compiled Dispatcher)**:
    - Route registration takes place in a mutable builder phase (`Router`, `RouteGroup`, `RouteResource`).
    - Dispatching requests operates on an immutable, thread-safe (`Send + Sync + 'static`) `CompiledRouter` generated
      by `Router::compile()`.
    - Route middleware chains must be pre-composed during compilation into `dhap::Pipeline` instances (`Global` ➔
      `Group (outer to inner)` ➔ `Route` ➔ `Terminal Handler`) stored in `poth::MethodMatcher` to avoid
      runtime hierarchy resolution overhead.
3. **Deterministic Precedence**:
    - Route compilation must rely on `poth::MethodMatcher` to guarantee deterministic specificity hierarchy
      regardless of registration order.
4. **Unified Handler Contract & `IntoResponse`**:
    - Handlers adhere to `async fn(&mut HttpContext) -> impl IntoResponse`.
    - `IntoResponse::apply(self, ctx: &mut HttpContext) -> Result<(), RouterError>` merges status, headers, and body
      into `ctx.response` without clobbering upstream middleware cookies or headers.
5. **Non-Destructive Buffered Request Bodies**:
    - Request payloads are buffered in memory as `Bytes` by default up to `max_body_size` (default 2MB) with
      `413 Payload Too Large` enforcement.
    - Handlers and middleware inspect bodies non-destructively: `&request.json::<T>()`, `&request.bytes()`, etc.
    - Explicit streaming is supported for large file uploads.
6. **Middleware Onion & SessionContext**:
    - Middleware execution must integrate with `dhap::Pipeline`.
    - `HttpContext` must implement `session::SessionContext` so `SessionMiddleware` attaches sessions seamlessly.
7. **Type-Safe Request Extensions**:
    - `HttpContext` must provide an `Extensions` type map (`TypeId`-indexed) for cross-cutting middleware data.
8. **Method Spoofing & Verb Resolution**:
    - Support method spoofing for `POST` via `_method` and `X-HTTP-Method-Override`. Expose `request.intended_method()`
      and `request.method()`.
9. **Reverse URL & Cryptographic Signing**:
    - Reverse URL generation must strictly validate required parameters and URL-encode dynamic values using
      `poth::MethodMatcher::build_path`.
    - Integrate `gopon::UrlSigner` into `UrlGenerator` and expose signature checks on `Request`.
10. **Automatic RFC 9110 Compliance & Error Mapping**:
    - Handle automatic `OPTIONS` (`204 No Content` + `Allow` header), automatic `HEAD` (fallback to `GET` + body
      suppression),
      automatic `405 Method Not Allowed` with `Allow` header, and custom SPA fallback via `router.fallback(handler)`.
    - `RouterError` must implement `IntoResponse` with standard HTTP status code mappings.
11. **Quad-Surface Routing Interoperability**:
    - Support the Fluent Builder (`Router`), Modular Nesting (`router.nest()`), and Declarative Macro (`routes!`) in
      this crate, ensuring all syntaxes compile down to the identical `Router` builder calls.
    - Provide registration compatibility for companion proc-macro attributes (`#[get]`, `#[controller]`).

