# druto-router

[![Crates.io](https://img.shields.io/crates/v/druto-router.svg)](https://crates.io/crates/druto-router)
[![Documentation](https://docs.rs/druto-router/badge.svg)](https://docs.rs/druto-router)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

High-performance HTTP routing, request/response lifecycle, middleware onion runner, and execution context for Druto.

Bridging `poth` (radix trie routing), `dhap` (asynchronous middleware onion), `chirkut` (RFC 6265bis cookies), `khonj` (nested query parsing), `gopon` (HMAC signed URLs), and `druto-session` (stateful session management), `druto-router` delivers an ergonomic and zero-allocation HTTP layer.

---

## Features

- **Two-Phase Routing Engine**:
  - Mutable builder phase (`Router`): fluent route registration, nested groups, resource controllers, and parameter constraints.
  - Immutable dispatch phase (`CompiledRouter`): pre-compiled route trees, flattened zero-allocation middleware stacks, and sub-microsecond path parameter lookups.
- **Quad-Surface Route Declaration**:
  - **Fluent Builder API**: `router.get(...)`, `router.post(...)`, `router.group(...)`, `router.resource(...)`.
  - **Modular Sub-Router Nesting**: `router.nest("/api/v1", api_router)`.
  - **Declarative Manifest Macro**: `routes! { ... }` compile-time DSL.
- **RESTful Resource Controllers (`ResourceController`)**:
  - Conventional 7-action routing (`index`, `create`, `store`, `show`, `edit`, `update`, `destroy`) with API-only filtering.
- **Unified Handler Contract & `IntoResponse`**:
  - Asynchronous handlers operating over `&mut HttpContext`.
  - Built-in implementations for `()`, `Response`, `&str`, `String`, `serde_json::Value`, `Redirect`, and `Result<T, E>`.
- **Automatic RFC 9110 Compliance**:
  - Automatic `OPTIONS` and `HEAD` negotiation.
  - Status `405 Method Not Allowed` with `Allow` header generation.
- **Reverse Routing & Signed URLs (`UrlGenerator`)**:
  - Named route URL reconstruction and HMAC tamper-proof link signing.
- **Schema Validation Integration**:
  - Seamless request validation and content-negotiated RFC 9457 error reporting powered by `jachai`.

---

## Installation

Add `druto-router` to your `Cargo.toml`:

```toml
[dependencies]
druto-router = "0.1.0"
```

---

## Quick Start

```rust
use druto_router::{HttpContext, IntoResponse, Method, Request, Router, routes};

async fn hello(ctx: &mut HttpContext) -> impl IntoResponse {
    let name = ctx.request.param("name").unwrap_or("World");
    format!("Hello, {}!", name)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut router = Router::new();
    router.get("/greet/:name", hello);

    let compiled = router.compile();

    let req = Request::builder()
        .method(Method::Get)
        .path("/greet/Developer")
        .build()?;

    let res = compiled.dispatch(req).await;
    assert_eq!(res.status().as_u16(), 200);

    Ok(())
}
```

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
