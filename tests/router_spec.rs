//! Comprehensive integration test suite for the router crate.

use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr},
    pin::Pin,
    sync::{Arc, atomic::Ordering},
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use bytes::Bytes;
use dhap::{BoxFuture as DhapBoxFuture, Error as DhapError, Middleware, Next};
use druto_router as router;
use druto_router::{
    BoxFuture, HeaderMap, HeaderName, HeaderValue, HttpContext, IntoResponse, Redirect, Request,
    ResourceAction, ResourceController, Response, ResponseBody, RouteResource, Router, RouterError,
    StatusCode, from_handler, routes,
};
use gopon::{Key, KeyRing, UrlSigner};
use poth::Method;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use session::{
    MemorySessionStore, SessionConfig, SessionContext, SessionManager, SessionMiddleware,
};

// --- Test Utilities (Zero-Dependency Executor) ---

fn noop_waker() -> Waker {
    fn noop_clone(data: *const ()) -> RawWaker {
        RawWaker::new(data, &NOOP_VTABLE)
    }
    fn noop_action(data: *const ()) {
        let _ = data;
    }
    static NOOP_VTABLE: RawWakerVTable =
        RawWakerVTable::new(noop_clone, noop_action, noop_action, noop_action);

    // SAFETY: RawWaker implementation performs no operations and maintains valid null pointer state.
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &NOOP_VTABLE)) }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut pinned = Pin::from(Box::new(future));
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);
    match pinned.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => {
            panic!("test future unexpectedly yielded Pending without an external driver")
        }
    }
}

// --- Unit Tests: StatusCode ---

#[test]
fn test_status_code_classifications_and_constants() {
    assert_eq!(StatusCode::OK.as_u16(), 200);
    assert_eq!(StatusCode::CREATED.as_u16(), 201);
    assert_eq!(StatusCode::NO_CONTENT.as_u16(), 204);
    assert_eq!(StatusCode::MOVED_PERMANENTLY.as_u16(), 301);
    assert_eq!(StatusCode::FOUND.as_u16(), 302);
    assert_eq!(StatusCode::SEE_OTHER.as_u16(), 303);
    assert_eq!(StatusCode::NOT_MODIFIED.as_u16(), 304);
    assert_eq!(StatusCode::TEMPORARY_REDIRECT.as_u16(), 307);
    assert_eq!(StatusCode::PERMANENT_REDIRECT.as_u16(), 308);
    assert_eq!(StatusCode::BAD_REQUEST.as_u16(), 400);
    assert_eq!(StatusCode::UNAUTHORIZED.as_u16(), 401);
    assert_eq!(StatusCode::FORBIDDEN.as_u16(), 403);
    assert_eq!(StatusCode::NOT_FOUND.as_u16(), 404);
    assert_eq!(StatusCode::METHOD_NOT_ALLOWED.as_u16(), 405);
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE.as_u16(), 413);
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY.as_u16(), 422);
    assert_eq!(StatusCode::TOO_MANY_REQUESTS.as_u16(), 429);
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR.as_u16(), 500);
    assert_eq!(StatusCode::NOT_IMPLEMENTED.as_u16(), 501);
    assert_eq!(StatusCode::BAD_GATEWAY.as_u16(), 502);
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE.as_u16(), 503);
    assert_eq!(StatusCode::GATEWAY_TIMEOUT.as_u16(), 504);

    // Classifications
    assert!(StatusCode::from_u16(100).is_informational());
    assert!(!StatusCode::from_u16(100).is_success());

    assert!(StatusCode::OK.is_success());
    assert!(!StatusCode::OK.is_informational());
    assert!(!StatusCode::OK.is_client_error());

    assert!(StatusCode::MOVED_PERMANENTLY.is_redirection());
    assert!(!StatusCode::MOVED_PERMANENTLY.is_success());

    assert!(StatusCode::NOT_FOUND.is_client_error());
    assert!(!StatusCode::NOT_FOUND.is_server_error());

    assert!(StatusCode::INTERNAL_SERVER_ERROR.is_server_error());
    assert!(!StatusCode::INTERNAL_SERVER_ERROR.is_client_error());

    // Canonical reasons
    assert_eq!(StatusCode::OK.canonical_reason(), Some("OK"));
    assert_eq!(StatusCode::NOT_FOUND.canonical_reason(), Some("Not Found"));
    assert_eq!(
        StatusCode::METHOD_NOT_ALLOWED.canonical_reason(),
        Some("Method Not Allowed")
    );
    assert_eq!(StatusCode::from_u16(999).canonical_reason(), None);

    // Display
    assert_eq!(format!("{}", StatusCode::OK), "200 OK");
    assert_eq!(format!("{}", StatusCode::NOT_FOUND), "404 Not Found");
    assert_eq!(format!("{}", StatusCode::from_u16(999)), "999");
}

// --- Unit Tests: HeaderName, HeaderValue, HeaderMap ---

#[test]
fn test_header_name_value_and_map() {
    let name_static = HeaderName::CONTENT_TYPE;
    let name_from_str = HeaderName::new("Content-Type");
    assert_eq!(name_static, name_from_str);
    assert_eq!(name_static.as_str(), "content-type");
    assert_eq!(format!("{name_static}"), "content-type");

    let val = HeaderValue::new("application/json");
    assert_eq!(val.as_str(), "application/json");
    assert_eq!(format!("{val}"), "application/json");

    let mut map = HeaderMap::new();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);

    map.insert(HeaderName::CONTENT_TYPE, "text/html");
    assert_eq!(map.len(), 1);
    assert!(!map.is_empty());
    assert!(map.contains_key(HeaderName::CONTENT_TYPE));
    assert_eq!(map.get(HeaderName::CONTENT_TYPE), Some("text/html"));

    // Case-insensitive lookup
    assert_eq!(map.get("CONTENT-TYPE"), Some("text/html"));
    assert_eq!(map.get("content-type"), Some("text/html"));

    // Overwrite
    map.insert(HeaderName::CONTENT_TYPE, "application/json");
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(HeaderName::CONTENT_TYPE), Some("application/json"));

    // Append multiple values for same key
    map.append(HeaderName::SET_COOKIE, "cookie1=val1");
    map.append(HeaderName::SET_COOKIE, "cookie2=val2");
    assert_eq!(map.len(), 2);
    let cookies: Vec<_> = map.get_all(HeaderName::SET_COOKIE).collect();
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0], "cookie1=val1");
    assert_eq!(cookies[1], "cookie2=val2");

    // Remove
    let removed = map.remove(HeaderName::CONTENT_TYPE);
    assert_eq!(
        removed.as_deref(),
        Some(&[HeaderValue::new("application/json")][..])
    );
    assert!(!map.contains_key(HeaderName::CONTENT_TYPE));
    assert_eq!(map.len(), 1);

    // Iter
    let pairs: Vec<_> = map.iter().collect();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0.as_str(), "set-cookie");
    assert_eq!(pairs[0].1.len(), 2);

    // FromIterator
    let collected_map: HeaderMap = vec![("X-First", "1"), ("X-Second", "2")]
        .into_iter()
        .collect();
    assert_eq!(collected_map.len(), 2);
    assert_eq!(collected_map.get("x-first"), Some("1"));
}

// --- Unit Tests: Extensions ---

#[derive(Debug, PartialEq, Eq)]
struct TestUserId(u64);

#[derive(Debug, PartialEq, Eq)]
struct TestRole(String);

#[test]
fn test_extensions_container() {
    let mut extensions = router::Extensions::new();
    assert!(extensions.get::<TestUserId>().is_none());
    assert!(!extensions.contains::<TestUserId>());

    extensions.insert(TestUserId(42));
    extensions.insert(TestRole("administrator".to_string()));

    assert!(extensions.contains::<TestUserId>());
    assert!(extensions.contains::<TestRole>());
    assert_eq!(extensions.get::<TestUserId>(), Some(&TestUserId(42)));
    assert_eq!(
        extensions.get::<TestRole>(),
        Some(&TestRole("administrator".to_string()))
    );

    // Mutate
    if let Some(user_id) = extensions.get_mut::<TestUserId>() {
        user_id.0 = 100;
    }
    assert_eq!(extensions.get::<TestUserId>(), Some(&TestUserId(100)));

    // Remove
    let removed_role = extensions.remove::<TestRole>();
    assert_eq!(removed_role, Some(TestRole("administrator".to_string())));
    assert!(!extensions.contains::<TestRole>());

    // Clear
    extensions.clear();
    assert!(!extensions.contains::<TestUserId>());
}

// --- Unit Tests: Response, ResponseBody, Redirect ---

#[test]
fn test_response_constructors_and_body() {
    let empty_body = ResponseBody::Empty;
    assert!(empty_body.is_empty());
    assert_eq!(empty_body.len(), 0);
    assert_eq!(empty_body.as_bytes(), Bytes::new());

    let bytes_body = ResponseBody::Bytes(Bytes::from_static(b"Hello Druto"));
    assert!(!bytes_body.is_empty());
    assert_eq!(bytes_body.len(), 11);
    assert_eq!(bytes_body.as_bytes(), Bytes::from_static(b"Hello Druto"));

    let text_body = ResponseBody::Text("hello text".to_string());
    assert_eq!(text_body.len(), 10);
    assert_eq!(text_body.as_bytes(), Bytes::from_static(b"hello text"));

    let html_body = ResponseBody::Html("<h1>hi</h1>".to_string());
    assert_eq!(html_body.len(), 11);

    let json_body = ResponseBody::Json("{\"k\":1}".to_string());
    assert_eq!(json_body.len(), 7);

    let mut res = Response::ok();
    assert_eq!(res.status(), StatusCode::OK);
    res.set_status(StatusCode::CREATED);
    assert_eq!(res.status(), StatusCode::CREATED);

    res.set_header(HeaderName::CONTENT_TYPE, "text/plain");
    assert_eq!(
        res.headers().get(HeaderName::CONTENT_TYPE),
        Some("text/plain")
    );

    res.append_header(HeaderName::ALLOW, "GET");
    res.append_header(HeaderName::ALLOW, "POST");
    assert_eq!(res.headers().get_all(HeaderName::ALLOW).count(), 2);

    res.text("plain content");
    assert_eq!(res.body().as_bytes(), Bytes::from_static(b"plain content"));
    assert_eq!(
        res.headers().get(HeaderName::CONTENT_TYPE),
        Some("text/plain; charset=utf-8")
    );

    let json_val = json!({ "key": "value" });
    res.json(&json_val).expect("json serialization failed");
    assert_eq!(
        res.headers().get(HeaderName::CONTENT_TYPE),
        Some("application/json; charset=utf-8")
    );

    res.html("<p>test</p>");
    assert_eq!(
        res.headers().get(HeaderName::CONTENT_TYPE),
        Some("text/html; charset=utf-8")
    );

    let mut redir_res = Response::ok();
    redir_res.redirect("/destination", None);
    assert_eq!(redir_res.status(), StatusCode::FOUND);
    assert_eq!(
        redir_res.headers().get(HeaderName::LOCATION),
        Some("/destination")
    );

    let redir = Redirect::to("/login");
    assert_eq!(redir.status(), StatusCode::FOUND);
    assert_eq!(redir.destination(), "/login");

    let perm_redir = Redirect::permanent("/home");
    assert_eq!(perm_redir.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(perm_redir.destination(), "/home");

    let see_other_redir = Redirect::see_other("/other");
    assert_eq!(see_other_redir.status(), StatusCode::SEE_OTHER);

    let temp_redir = Redirect::temporary("/temp");
    assert_eq!(temp_redir.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(temp_redir.destination(), "/temp");

    let chained_res = Response::new(StatusCode::ACCEPTED)
        .with_status(StatusCode::CREATED)
        .header("X-App", "Druto")
        .with_body(ResponseBody::Bytes(Bytes::from_static(b"data")));
    assert_eq!(chained_res.status(), StatusCode::CREATED);
    assert_eq!(chained_res.headers().get("x-app"), Some("Druto"));
    assert_eq!(chained_res.body().as_bytes(), Bytes::from_static(b"data"));
}

// --- Unit Tests: IntoResponse implementations ---

#[test]
fn test_into_response_contracts() {
    let mut ctx = HttpContext::new(Request::builder().path("/").build());

    // () preserves existing
    ().apply(&mut ctx).expect("unit into response");
    assert_eq!(ctx.response.status(), StatusCode::OK);

    // &'static str
    "static str response".apply(&mut ctx).expect("str apply");
    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"static str response")
    );
    assert_eq!(
        ctx.response.headers().get(HeaderName::CONTENT_TYPE),
        Some("text/plain; charset=utf-8")
    );

    // String
    "owned string response"
        .to_string()
        .apply(&mut ctx)
        .expect("string apply");
    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"owned string response")
    );

    // serde_json::Value
    json!({ "success": true })
        .apply(&mut ctx)
        .expect("json apply");
    assert_eq!(
        ctx.response.headers().get(HeaderName::CONTENT_TYPE),
        Some("application/json; charset=utf-8")
    );

    // StatusCode
    StatusCode::ACCEPTED.apply(&mut ctx).expect("status apply");
    assert_eq!(ctx.response.status(), StatusCode::ACCEPTED);

    // (StatusCode, T)
    (StatusCode::CREATED, "created item")
        .apply(&mut ctx)
        .expect("tuple apply");
    assert_eq!(ctx.response.status(), StatusCode::CREATED);
    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"created item")
    );

    // Redirect
    Redirect::to("/dashboard")
        .apply(&mut ctx)
        .expect("redirect apply");
    assert_eq!(ctx.response.status(), StatusCode::FOUND);
    assert_eq!(
        ctx.response.headers().get(HeaderName::LOCATION),
        Some("/dashboard")
    );

    // Response merge
    let mut custom_res = Response::new(StatusCode::PARTIAL_CONTENT);
    custom_res.set_header("X-Custom-Header", "druto-value");
    custom_res.text("custom body");
    custom_res.apply(&mut ctx).expect("response apply");
    assert_eq!(ctx.response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        ctx.response.headers().get("X-Custom-Header"),
        Some("druto-value")
    );
    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"custom body")
    );

    // Result::Ok
    let ok_res: Result<&'static str, StatusCode> = Ok("result ok");
    ok_res.apply(&mut ctx).expect("ok apply");
    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"result ok")
    );

    // Result::Err
    let err_res: Result<&'static str, StatusCode> = Err(StatusCode::BAD_REQUEST);
    err_res.apply(&mut ctx).expect("err apply");
    assert_eq!(ctx.response.status(), StatusCode::BAD_REQUEST);
}

// --- Request Parsing, Proxies, and Method Spoofing Tests ---

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct FilterQuery {
    page: u32,
    tag: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct CreateUserPayload {
    username: String,
    email: String,
}

#[test]
fn test_request_query_json_and_form_parsing() {
    // Query string deserialization
    let req = Request::builder()
        .path("/items")
        .query("page=2&tag=druto")
        .build();
    let query: FilterQuery = req.query_parsed().expect("query parse");
    assert_eq!(
        query,
        FilterQuery {
            page: 2,
            tag: "druto".to_string(),
        }
    );

    // JSON body parsing
    let payload = CreateUserPayload {
        username: "ferris".to_string(),
        email: "ferris@rust.lang".to_string(),
    };
    let json_req = Request::builder()
        .path("/users")
        .json(&payload)
        .expect("serialize json")
        .build();
    let parsed: CreateUserPayload = json_req.json().expect("deserialize json");
    assert_eq!(parsed, payload);
    assert!(json_req.is_json());

    // Form urlencoded parsing
    let form_req = Request::builder()
        .path("/submit")
        .body(Bytes::from_static(b"username=crab&email=crab%40rust.lang"))
        .build();
    let form_data: CreateUserPayload = form_req.form().expect("deserialize form");
    assert_eq!(
        form_data,
        CreateUserPayload {
            username: "crab".to_string(),
            email: "crab@rust.lang".to_string(),
        }
    );

    // RouteParams typed parsing
    let mut params_req = Request::builder().path("/users/42").build();
    params_req.params_mut().insert("id", "42");
    assert_eq!(params_req.param("id"), Some("42"));
    assert_eq!(params_req.param_parsed::<u64>("id"), Some(Ok(42)));
    assert_eq!(params_req.param("unknown"), None);
}

#[test]
fn test_proxy_ip_resolution_and_security() {
    let direct_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

    // When trust_proxy is false, ignore forwarded headers
    let untrusted_req = Request::builder()
        .client_ip(direct_ip)
        .header("X-Forwarded-For", "203.0.113.195, 70.41.3.18")
        .trust_proxy(false)
        .build();
    assert_eq!(untrusted_req.ip(), Some("192.168.1.100".to_string()));
    assert!(!untrusted_req.is_secure());

    // When trust_proxy is true, honor X-Forwarded-For
    let trusted_req = Request::builder()
        .client_ip(direct_ip)
        .header("X-Forwarded-For", "203.0.113.195, 70.41.3.18")
        .header("X-Forwarded-Proto", "https")
        .header("X-Forwarded-Host", "druto.dev")
        .trust_proxy(true)
        .build();
    assert_eq!(trusted_req.ip(), Some("203.0.113.195".to_string()));
    assert!(trusted_req.is_secure());
    assert_eq!(trusted_req.host(), Some("druto.dev"));

    // Forwarded standard header (RFC 7239)
    let rfc_req = Request::builder()
        .client_ip(direct_ip)
        .header("Forwarded", "for=198.51.100.1;proto=https")
        .trust_proxy(true)
        .build();
    assert_eq!(rfc_req.ip(), Some("198.51.100.1".to_string()));

    // X-Real-IP fallback
    let real_ip_req = Request::builder()
        .client_ip(direct_ip)
        .header("X-Real-IP", "198.51.100.77")
        .trust_proxy(true)
        .build();
    assert_eq!(real_ip_req.ip(), Some("198.51.100.77".to_string()));
}

#[test]
fn test_http_method_spoofing() {
    // 1. Spoofing disabled: POST remains POST even with _method or headers
    let mut req1 = Request::builder()
        .method(Method::Post)
        .header("X-HTTP-Method-Override", "DELETE")
        .query("_method=DELETE")
        .body(Bytes::from_static(b"_method=DELETE"))
        .build();
    req1.resolve_spoofed_method(false);
    assert_eq!(req1.method(), &Method::Post);
    assert_eq!(req1.intended_method(), &Method::Post);

    // 2. Spoofing enabled with X-HTTP-Method-Override
    let mut req2 = Request::builder()
        .method(Method::Post)
        .header("X-HTTP-Method-Override", "PUT")
        .build();
    req2.resolve_spoofed_method(true);
    assert_eq!(req2.method(), &Method::Put);
    assert_eq!(req2.intended_method(), &Method::Post);

    // 3. Spoofing enabled with query parameter
    let mut req3 = Request::builder()
        .method(Method::Post)
        .query("_method=DELETE&other=1")
        .build();
    req3.resolve_spoofed_method(true);
    assert_eq!(req3.method(), &Method::Delete);

    // 4. Spoofing enabled with form body
    let mut req4 = Request::builder()
        .method(Method::Post)
        .body(Bytes::from_static(b"_method=PATCH&title=updated"))
        .build();
    req4.resolve_spoofed_method(true);
    assert_eq!(req4.method(), &Method::Patch);

    // 5. Non-POST requests never spoofed
    let mut req5 = Request::builder()
        .method(Method::Get)
        .header("X-HTTP-Method-Override", "DELETE")
        .query("_method=DELETE")
        .build();
    req5.resolve_spoofed_method(true);
    assert_eq!(req5.method(), &Method::Get);
}

// --- Signed URL Generation and Verification Tests ---

#[test]
fn test_signed_urls_and_request_verification() {
    let key = Key::generate().expect("key generation failed");
    let signer = UrlSigner::new(key);

    let mut router = Router::new();
    router.config_mut().url_signer = Some(signer.clone());
    router
        .get(
            "/reports/:id",
            from_handler(|ctx| {
                let _ = ctx;
                Box::pin(async move { Ok(()) })
            }),
        )
        .name("reports.show");
    let compiled = router.compile().expect("compile router");

    // Relative signed URL
    let signed_relative = compiled
        .url_generator()
        .signed_url_for("reports.show", &[("id", "42")])
        .expect("sign url");
    assert!(signed_relative.starts_with("/reports/42?signature="));

    let split_pos = signed_relative.find('?').expect("query string");
    let path = &signed_relative[..split_pos];
    let query = &signed_relative[split_pos + 1..];

    let valid_req = Request::builder().path(path).query(query).build();
    assert!(valid_req.has_valid_signature(&signer));

    // Tampered query signature fails
    let tampered_req = Request::builder()
        .path(path)
        .query("signature=tampered_signature_value")
        .build();
    assert!(!tampered_req.has_valid_signature(&signer));

    // Missing route error in url generator
    let missing_route_err = compiled
        .url_generator()
        .url_for("nonexistent.route", &[("id", "42")]);
    assert!(matches!(missing_route_err, Err(RouterError::NotFound)));

    // Missing parameter error in url generator
    let missing_param_err = compiled.url_generator().url_for("reports.show", &[]);
    assert!(matches!(
        missing_param_err,
        Err(RouterError::InvalidParams { .. })
    ));
}

// --- Router Core Dispatch Tests: Verbs, Params, Regex, Groups, Nesting ---

#[test]
fn test_router_verbs_and_parameter_extraction() {
    let mut router = Router::new();

    router
        .get(
            "/users/:id",
            from_handler(|ctx| {
                let id = ctx.request.param("id").unwrap_or("").to_string();
                Box::pin(async move {
                    ctx.response.text(format!("user {id}"));
                    Ok(())
                })
            }),
        )
        .name("users.show");

    router.post(
        "/users",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.set_status(StatusCode::CREATED);
                ctx.response.text("created user");
                Ok(())
            })
        }),
    );

    router.put(
        "/users/:id",
        from_handler(|ctx| {
            let id = ctx.request.param("id").unwrap_or("").to_string();
            Box::pin(async move {
                ctx.response.text(format!("updated user {id}"));
                Ok(())
            })
        }),
    );

    router.delete(
        "/users/:id",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.set_status(StatusCode::NO_CONTENT);
                Ok(())
            })
        }),
    );

    router.patch(
        "/items/:id",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("patched item");
                Ok(())
            })
        }),
    );

    router.any(
        "/wildcard/*path",
        from_handler(|ctx| {
            let path = ctx.request.param("path").unwrap_or("").to_string();
            Box::pin(async move {
                ctx.response.text(format!("wildcard {path}"));
                Ok(())
            })
        }),
    );

    let compiled = router.compile().expect("compile");

    // Test GET /users/123
    let mut get_ctx = HttpContext::new(Request::builder().path("/users/123").build());
    block_on(compiled.dispatch(&mut get_ctx)).expect("dispatch get");
    assert_eq!(get_ctx.response.status(), StatusCode::OK);
    assert_eq!(
        get_ctx.response.body().as_bytes(),
        Bytes::from_static(b"user 123")
    );
    assert_eq!(get_ctx.matched_route_name(), Some("users.show"));

    // Test POST /users
    let mut post_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Post)
            .path("/users")
            .build(),
    );
    block_on(compiled.dispatch(&mut post_ctx)).expect("dispatch post");
    assert_eq!(post_ctx.response.status(), StatusCode::CREATED);

    // Test PUT /users/123
    let mut put_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Put)
            .path("/users/123")
            .build(),
    );
    block_on(compiled.dispatch(&mut put_ctx)).expect("dispatch put");
    assert_eq!(
        put_ctx.response.body().as_bytes(),
        Bytes::from_static(b"updated user 123")
    );

    // Test DELETE /users/123
    let mut del_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Delete)
            .path("/users/123")
            .build(),
    );
    block_on(compiled.dispatch(&mut del_ctx)).expect("dispatch del");
    assert_eq!(del_ctx.response.status(), StatusCode::NO_CONTENT);

    // Test Wildcard
    let mut wild_ctx = HttpContext::new(
        Request::builder()
            .path("/wildcard/sub/dir/file.txt")
            .build(),
    );
    block_on(compiled.dispatch(&mut wild_ctx)).expect("dispatch wild");
    assert_eq!(
        wild_ctx.response.body().as_bytes(),
        Bytes::from_static(b"wildcard sub/dir/file.txt")
    );
}

#[test]
fn test_parameter_regex_constraints() {
    let mut router = Router::new();
    router
        .get(
            "/articles/:id",
            from_handler(|ctx| {
                Box::pin(async move {
                    ctx.response.text("numeric article");
                    Ok(())
                })
            }),
        )
        .where_num("id");

    let compiled = router.compile().expect("compile");

    // Matching numeric id
    let mut valid_ctx = HttpContext::new(Request::builder().path("/articles/42").build());
    block_on(compiled.dispatch(&mut valid_ctx)).expect("dispatch valid");
    assert_eq!(valid_ctx.response.status(), StatusCode::OK);

    // Non-numeric id yields 404
    let mut invalid_ctx = HttpContext::new(Request::builder().path("/articles/slug-name").build());
    block_on(compiled.dispatch(&mut invalid_ctx)).expect("dispatch invalid");
    assert_eq!(invalid_ctx.response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_route_groups_and_modular_sub_router_nesting() {
    let mut router = Router::new();

    // Route Group
    router.group_prefixed("/admin", |admin| {
        admin.name("admin.");
        admin
            .get(
                "/dashboard",
                from_handler(|ctx| {
                    Box::pin(async move {
                        ctx.response.text("admin dashboard");
                        Ok(())
                    })
                }),
            )
            .name("dashboard");

        admin.group_prefixed("/settings", |settings| {
            settings.get(
                "/general",
                from_handler(|ctx| {
                    Box::pin(async move {
                        ctx.response.text("general settings");
                        Ok(())
                    })
                }),
            );
        });
    });

    // Sub-router nesting
    let mut sub_router = Router::new();
    sub_router
        .get(
            "/status",
            from_handler(|ctx| {
                Box::pin(async move {
                    ctx.response.text("v1 ok");
                    Ok(())
                })
            }),
        )
        .name("status");
    router.nest_as("/api/v1", "v1.", sub_router);

    // Static redirect
    router.redirect("/legacy", "/modern", StatusCode::MOVED_PERMANENTLY);

    let compiled = router.compile().expect("compile");

    // Group /admin/dashboard
    let mut admin_ctx = HttpContext::new(Request::builder().path("/admin/dashboard").build());
    block_on(compiled.dispatch(&mut admin_ctx)).expect("dispatch admin");
    assert_eq!(
        admin_ctx.response.body().as_bytes(),
        Bytes::from_static(b"admin dashboard")
    );
    assert_eq!(admin_ctx.matched_route_name(), Some("admin.dashboard"));

    // Nested group /admin/settings/general
    let mut nested_ctx =
        HttpContext::new(Request::builder().path("/admin/settings/general").build());
    block_on(compiled.dispatch(&mut nested_ctx)).expect("dispatch nested");
    assert_eq!(
        nested_ctx.response.body().as_bytes(),
        Bytes::from_static(b"general settings")
    );

    // Nested sub-router /api/v1/status
    let mut sub_ctx = HttpContext::new(Request::builder().path("/api/v1/status").build());
    block_on(compiled.dispatch(&mut sub_ctx)).expect("dispatch sub");
    assert_eq!(
        sub_ctx.response.body().as_bytes(),
        Bytes::from_static(b"v1 ok")
    );
    assert_eq!(sub_ctx.matched_route_name(), Some("v1.status"));

    // Static redirect
    let mut redir_ctx = HttpContext::new(Request::builder().path("/legacy").build());
    block_on(compiled.dispatch(&mut redir_ctx)).expect("dispatch redir");
    assert_eq!(redir_ctx.response.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        redir_ctx.response.headers().get(HeaderName::LOCATION),
        Some("/modern")
    );
}

// --- Resource Controller Tests ---

struct PostResourceController;

impl ResourceController for PostResourceController {
    fn index<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        Box::pin(async move {
            ctx.response.text("posts.index");
            Ok(())
        })
    }

    fn create<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        Box::pin(async move {
            ctx.response.text("posts.create");
            Ok(())
        })
    }

    fn store<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        Box::pin(async move {
            ctx.response.set_status(StatusCode::CREATED);
            ctx.response.text("posts.store");
            Ok(())
        })
    }

    fn show<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        let id = ctx.request.param("id").unwrap_or("").to_string();
        Box::pin(async move {
            ctx.response.text(format!("posts.show {id}"));
            Ok(())
        })
    }

    fn edit<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        Box::pin(async move {
            ctx.response.text("posts.edit");
            Ok(())
        })
    }

    fn update<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        Box::pin(async move {
            ctx.response.text("posts.update");
            Ok(())
        })
    }

    fn destroy<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        Box::pin(async move {
            ctx.response.text("posts.destroy");
            Ok(())
        })
    }
}

#[test]
fn test_resource_controller_actions_and_api_only() {
    let mut router = Router::new();
    router.resource("posts", PostResourceController);

    // API only controller excludes create and edit
    struct ApiCommentsController;
    impl ResourceController for ApiCommentsController {
        fn index<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
            Box::pin(async move {
                ctx.response.text("comments.index");
                Ok(())
            })
        }
    }
    router.resource_with("comments", ApiCommentsController, |res| {
        res.api_only();
    });

    let compiled = router.compile().expect("compile resource router");

    // 1. GET /posts -> index
    let mut ctx1 = HttpContext::new(Request::builder().path("/posts").build());
    block_on(compiled.dispatch(&mut ctx1)).expect("dispatch index");
    assert_eq!(
        ctx1.response.body().as_bytes(),
        Bytes::from_static(b"posts.index")
    );
    assert_eq!(ctx1.matched_route_name(), Some("posts.index"));

    // 2. GET /posts/create -> create
    let mut ctx2 = HttpContext::new(Request::builder().path("/posts/create").build());
    block_on(compiled.dispatch(&mut ctx2)).expect("dispatch create");
    assert_eq!(
        ctx2.response.body().as_bytes(),
        Bytes::from_static(b"posts.create")
    );

    // 3. POST /posts -> store
    let mut ctx3 = HttpContext::new(
        Request::builder()
            .method(Method::Post)
            .path("/posts")
            .build(),
    );
    block_on(compiled.dispatch(&mut ctx3)).expect("dispatch store");
    assert_eq!(ctx3.response.status(), StatusCode::CREATED);
    assert_eq!(
        ctx3.response.body().as_bytes(),
        Bytes::from_static(b"posts.store")
    );

    // 4. GET /posts/99 -> show
    let mut ctx4 = HttpContext::new(Request::builder().path("/posts/99").build());
    block_on(compiled.dispatch(&mut ctx4)).expect("dispatch show");
    assert_eq!(
        ctx4.response.body().as_bytes(),
        Bytes::from_static(b"posts.show 99")
    );

    // 5. GET /posts/99/edit -> edit
    let mut ctx5 = HttpContext::new(Request::builder().path("/posts/99/edit").build());
    block_on(compiled.dispatch(&mut ctx5)).expect("dispatch edit");
    assert_eq!(
        ctx5.response.body().as_bytes(),
        Bytes::from_static(b"posts.edit")
    );

    // 6. PUT /posts/99 -> update
    let mut ctx6 = HttpContext::new(
        Request::builder()
            .method(Method::Put)
            .path("/posts/99")
            .build(),
    );
    block_on(compiled.dispatch(&mut ctx6)).expect("dispatch update");
    assert_eq!(
        ctx6.response.body().as_bytes(),
        Bytes::from_static(b"posts.update")
    );

    // 7. DELETE /posts/99 -> destroy
    let mut ctx7 = HttpContext::new(
        Request::builder()
            .method(Method::Delete)
            .path("/posts/99")
            .build(),
    );
    block_on(compiled.dispatch(&mut ctx7)).expect("dispatch destroy");
    assert_eq!(
        ctx7.response.body().as_bytes(),
        Bytes::from_static(b"posts.destroy")
    );

    // api_only verifies comments/edit does not exist
    let mut ctx_edit = HttpContext::new(Request::builder().path("/comments/99/edit").build());
    block_on(compiled.dispatch(&mut ctx_edit)).expect("dispatch api_only edit");
    assert_eq!(ctx_edit.response.status(), StatusCode::NOT_FOUND);
}

// --- RFC 9110 Compliance Tests: OPTIONS, HEAD, 405 Method Not Allowed ---

#[test]
fn test_rfc9110_compliance_features() {
    let mut router = Router::new();

    router.get(
        "/rfc-endpoint",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("rfc body content");
                Ok(())
            })
        }),
    );

    router.post(
        "/rfc-endpoint",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.set_status(StatusCode::CREATED);
                Ok(())
            })
        }),
    );

    let compiled = router.compile().expect("compile");

    // 1. Automatic OPTIONS: returns 204 No Content with Allow header
    let mut opt_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Options)
            .path("/rfc-endpoint")
            .build(),
    );
    block_on(compiled.dispatch(&mut opt_ctx)).expect("dispatch options");
    assert_eq!(opt_ctx.response.status(), StatusCode::NO_CONTENT);
    assert_eq!(opt_ctx.response.body().as_bytes(), Bytes::new());
    let allow = opt_ctx
        .response
        .headers()
        .get(HeaderName::ALLOW)
        .expect("allow header");
    assert!(allow.contains("GET"));
    assert!(allow.contains("POST"));
    assert!(allow.contains("OPTIONS"));
    assert!(allow.contains("HEAD"));

    // 2. Automatic HEAD: executes GET handler, sets status/headers, suppresses body
    let mut head_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Head)
            .path("/rfc-endpoint")
            .build(),
    );
    block_on(compiled.dispatch(&mut head_ctx)).expect("dispatch head");
    assert_eq!(head_ctx.response.status(), StatusCode::OK);
    assert_eq!(
        head_ctx.response.headers().get(HeaderName::CONTENT_TYPE),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(head_ctx.response.body().as_bytes(), Bytes::new());

    // 3. Automatic 405 Method Not Allowed: method not supported on existing route
    let mut put_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Put)
            .path("/rfc-endpoint")
            .build(),
    );
    block_on(compiled.dispatch(&mut put_ctx)).expect("dispatch 405");
    assert_eq!(put_ctx.response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let allow_put = put_ctx
        .response
        .headers()
        .get(HeaderName::ALLOW)
        .expect("allow header");
    assert!(allow_put.contains("GET"));
    assert!(allow_put.contains("POST"));
}

// --- Middleware Onion, Extensions, and Session Integration Tests ---

struct OrderTrackingMiddleware {
    id: usize,
    order: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Middleware<HttpContext, Response, RouterError> for OrderTrackingMiddleware {
    fn handle<'a>(
        &'a self,
        ctx: &'a mut HttpContext,
        next: Next<HttpContext, Response, RouterError>,
    ) -> DhapBoxFuture<'a, Result<Response, DhapError<RouterError>>> {
        Box::pin(async move {
            self.order
                .lock()
                .unwrap()
                .push(format!("enter {}", self.id));
            let res = next.run(ctx).await?;
            self.order.lock().unwrap().push(format!("exit {}", self.id));
            Ok(res)
        })
    }
}

#[test]
fn test_middleware_onion_execution_order() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let mut router = Router::new();
    router.middleware(OrderTrackingMiddleware {
        id: 1,
        order: Arc::clone(&order),
    });

    router
        .get(
            "/onion",
            from_handler(|ctx| {
                Box::pin(async move {
                    ctx.response.text("inside handler");
                    Ok(())
                })
            }),
        )
        .middleware(OrderTrackingMiddleware {
            id: 2,
            order: Arc::clone(&order),
        });

    let compiled = router.compile().expect("compile");
    let mut ctx = HttpContext::new(Request::builder().path("/onion").build());
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch onion");

    let history = order.lock().unwrap().clone();
    assert_eq!(
        history,
        vec![
            "enter 1".to_string(),
            "enter 2".to_string(),
            "exit 2".to_string(),
            "exit 1".to_string(),
        ]
    );
}

struct ShortCircuitAuthMiddleware;

impl Middleware<HttpContext, Response, RouterError> for ShortCircuitAuthMiddleware {
    fn handle<'a>(
        &'a self,
        ctx: &'a mut HttpContext,
        next: Next<HttpContext, Response, RouterError>,
    ) -> DhapBoxFuture<'a, Result<Response, DhapError<RouterError>>> {
        let _ = next;
        Box::pin(async move {
            ctx.response.set_status(StatusCode::UNAUTHORIZED);
            ctx.response.text("unauthorized access");
            Ok(ctx.response.clone())
        })
    }
}

#[test]
fn test_middleware_short_circuiting() {
    let mut router = Router::new();
    router
        .get(
            "/protected",
            from_handler(|ctx| {
                Box::pin(async move {
                    ctx.response.text("secret data");
                    Ok(())
                })
            }),
        )
        .middleware(ShortCircuitAuthMiddleware);

    let compiled = router.compile().expect("compile");
    let mut ctx = HttpContext::new(Request::builder().path("/protected").build());
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch auth");

    assert_eq!(ctx.response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"unauthorized access")
    );
}

#[test]
fn test_extensions_propagation_from_middleware_to_handler() {
    struct AttachUserMiddleware;

    impl Middleware<HttpContext, Response, RouterError> for AttachUserMiddleware {
        fn handle<'a>(
            &'a self,
            ctx: &'a mut HttpContext,
            next: Next<HttpContext, Response, RouterError>,
        ) -> DhapBoxFuture<'a, Result<Response, DhapError<RouterError>>> {
            Box::pin(async move {
                ctx.extensions.insert(TestUserId(777));
                next.run(ctx).await
            })
        }
    }

    let mut router = Router::new();
    router
        .get(
            "/user-info",
            from_handler(|ctx| {
                let user_id = ctx.extensions.get::<TestUserId>().map(|u| u.0).unwrap_or(0);
                Box::pin(async move {
                    ctx.response.text(format!("user {user_id}"));
                    Ok(())
                })
            }),
        )
        .middleware(AttachUserMiddleware);

    let compiled = router.compile().expect("compile");
    let mut ctx = HttpContext::new(Request::builder().path("/user-info").build());
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch info");

    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"user 777")
    );
}

#[test]
fn test_session_middleware_integration_and_cookie_flush() {
    let store = Arc::new(MemorySessionStore::new());
    let session_config = SessionConfig::default();
    let manager = Arc::new(SessionManager::new(session_config, store));

    let mut router = Router::new();
    router.middleware(SessionMiddleware::new(Arc::clone(&manager)));

    router.get(
        "/set-session",
        from_handler(|ctx| {
            Box::pin(async move {
                if let Some(session) = ctx.session_mut() {
                    session.put("username", "druto_coder").expect("session put");
                    session.flash("notice", "logged in").expect("flash put");
                }
                ctx.response.text("session saved");
                Ok(())
            })
        }),
    );

    let compiled = router.compile().expect("compile session router");
    let mut ctx = HttpContext::new(Request::builder().path("/set-session").build());
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch session");

    assert_eq!(ctx.response.status(), StatusCode::OK);
    // Staged cookies flushed to response Set-Cookie headers
    let set_cookies: Vec<_> = ctx
        .response
        .headers()
        .get_all(HeaderName::SET_COOKIE)
        .collect();
    assert!(!set_cookies.is_empty());
    assert!(set_cookies[0].contains("druto_session="));
}

// --- Fallback, Custom Error Handler, and Payload Too Large Tests ---

#[test]
fn test_fallback_handler() {
    let mut router = Router::new();
    router.get(
        "/regular",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    router.fallback(from_handler(|ctx| {
        Box::pin(async move {
            ctx.response.set_status(StatusCode::NOT_FOUND);
            ctx.response.text("custom fallback 404 page");
            Ok(())
        })
    }));

    let compiled = router.compile().expect("compile fallback");
    let mut ctx = HttpContext::new(Request::builder().path("/does-not-exist").build());
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch fallback");

    assert_eq!(ctx.response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        ctx.response.body().as_bytes(),
        Bytes::from_static(b"custom fallback 404 page")
    );
}

#[test]
fn test_custom_error_handler_and_wants_json_formatting() {
    let mut router = Router::new();
    router.get(
        "/error",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(
                async move { Err(RouterError::Internal("simulated server crash".to_string())) },
            )
        }),
    );

    let error_triggered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&error_triggered);

    router.error_handler(move |ctx, err| {
        let flag = Arc::clone(&flag);
        Box::pin(async move {
            flag.store(true, Ordering::SeqCst);
            ctx.response.set_status(StatusCode::SERVICE_UNAVAILABLE);
            ctx.response.text(format!("Custom handled: {err}"));
            Ok(())
        })
    });

    let compiled = router.compile().expect("compile error handler");
    let mut ctx = HttpContext::new(Request::builder().path("/error").build());
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch err");

    assert!(error_triggered.load(Ordering::SeqCst));
    assert_eq!(ctx.response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        ctx.response
            .body()
            .as_bytes()
            .starts_with(b"Custom handled: 500 Internal Server Error")
    );
}

#[test]
fn test_payload_too_large_enforcement() {
    let mut router = Router::new();
    router.config_mut().max_body_size = 10; // 10 bytes limit
    router.post(
        "/upload",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("uploaded");
                Ok(())
            })
        }),
    );

    let compiled = router.compile().expect("compile limit router");

    // Body exceeding limit
    let large_body = Bytes::from_static(b"this is more than ten bytes payload");
    let mut ctx = HttpContext::new(
        Request::builder()
            .method(Method::Post)
            .path("/upload")
            .body(large_body)
            .build(),
    );
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch limit");

    assert_eq!(ctx.response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn test_wants_json_error_structure() {
    let router = Router::new();
    let compiled = router.compile().expect("compile empty");

    let mut ctx = HttpContext::new(
        Request::builder()
            .path("/not-found-json")
            .header(HeaderName::ACCEPT, "application/json")
            .build(),
    );
    block_on(compiled.dispatch(&mut ctx)).expect("dispatch json err");

    assert_eq!(ctx.response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        ctx.response.headers().get(HeaderName::CONTENT_TYPE),
        Some("application/json; charset=utf-8")
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&ctx.response.body().as_bytes()).expect("valid error json");
    assert_eq!(parsed["error"]["status"], 404);
}

// --- Declarative routes! Macro Tests ---

fn macro_handler(ctx: &mut HttpContext) -> BoxFuture<'_, Result<(), RouterError>> {
    Box::pin(async move {
        ctx.response.text("macro test");
        Ok(())
    })
}

#[test]
fn test_routes_macro_comprehensive() {
    let mut sub_router = Router::new();
    sub_router.get("/sub-path", macro_handler);

    let my_router = routes! {
        GET "/hello" => macro_handler,
        POST "/users" => macro_handler, as: "users.create",
        PUT "/users/:id" => macro_handler,
        DELETE "/users/:id" => macro_handler,
        RESOURCE "articles" => PostResourceController, api_only,
        NEST "/nested" => sub_router,
        REDIRECT "/old-link" => "/new-link",
        FALLBACK => macro_handler,
        GROUP "/admin" => {
            GET "/metrics" => macro_handler,
        },
    };

    let compiled = my_router.compile().expect("compile macro router");

    // Verify GET /hello
    let mut ctx1 = HttpContext::new(Request::builder().path("/hello").build());
    block_on(compiled.dispatch(&mut ctx1)).expect("macro hello");
    assert_eq!(
        ctx1.response.body().as_bytes(),
        Bytes::from_static(b"macro test")
    );

    // Verify POST /users with name
    let mut ctx2 = HttpContext::new(
        Request::builder()
            .method(Method::Post)
            .path("/users")
            .build(),
    );
    block_on(compiled.dispatch(&mut ctx2)).expect("macro post");
    assert_eq!(ctx2.matched_route_name(), Some("users.create"));

    // Verify GROUP /admin/metrics
    let mut ctx3 = HttpContext::new(Request::builder().path("/admin/metrics").build());
    block_on(compiled.dispatch(&mut ctx3)).expect("macro admin");
    assert_eq!(
        ctx3.response.body().as_bytes(),
        Bytes::from_static(b"macro test")
    );

    // Verify NEST /nested/sub-path
    let mut ctx4 = HttpContext::new(Request::builder().path("/nested/sub-path").build());
    block_on(compiled.dispatch(&mut ctx4)).expect("macro nest");
    assert_eq!(
        ctx4.response.body().as_bytes(),
        Bytes::from_static(b"macro test")
    );

    // Verify REDIRECT /old-link
    let mut ctx5 = HttpContext::new(Request::builder().path("/old-link").build());
    block_on(compiled.dispatch(&mut ctx5)).expect("macro redir");
    assert_eq!(ctx5.response.status(), StatusCode::FOUND);
    assert_eq!(
        ctx5.response.headers().get(HeaderName::LOCATION),
        Some("/new-link")
    );
}

// --- Exhaustive Coverage Tests ---

#[test]
fn test_status_codes_all_canonical_reasons() {
    let cases = [
        (100, Some("Continue")),
        (101, Some("Switching Protocols")),
        (102, Some("Processing")),
        (103, Some("Early Hints")),
        (200, Some("OK")),
        (201, Some("Created")),
        (202, Some("Accepted")),
        (203, Some("Non-Authoritative Information")),
        (204, Some("No Content")),
        (205, Some("Reset Content")),
        (206, Some("Partial Content")),
        (207, Some("Multi-Status")),
        (208, Some("Already Reported")),
        (226, Some("IM Used")),
        (300, Some("Multiple Choices")),
        (301, Some("Moved Permanently")),
        (302, Some("Found")),
        (303, Some("See Other")),
        (304, Some("Not Modified")),
        (305, Some("Use Proxy")),
        (307, Some("Temporary Redirect")),
        (308, Some("Permanent Redirect")),
        (400, Some("Bad Request")),
        (401, Some("Unauthorized")),
        (402, Some("Payment Required")),
        (403, Some("Forbidden")),
        (404, Some("Not Found")),
        (405, Some("Method Not Allowed")),
        (406, Some("Not Acceptable")),
        (407, Some("Proxy Authentication Required")),
        (408, Some("Request Timeout")),
        (409, Some("Conflict")),
        (410, Some("Gone")),
        (411, Some("Length Required")),
        (412, Some("Precondition Failed")),
        (413, Some("Payload Too Large")),
        (414, Some("URI Too Long")),
        (415, Some("Unsupported Media Type")),
        (416, Some("Range Not Satisfiable")),
        (417, Some("Expectation Failed")),
        (418, Some("I'm a teapot")),
        (421, Some("Misdirected Request")),
        (422, Some("Unprocessable Entity")),
        (423, Some("Locked")),
        (424, Some("Failed Dependency")),
        (425, Some("Too Early")),
        (426, Some("Upgrade Required")),
        (428, Some("Precondition Required")),
        (429, Some("Too Many Requests")),
        (431, Some("Request Header Fields Too Large")),
        (451, Some("Unavailable For Legal Reasons")),
        (500, Some("Internal Server Error")),
        (501, Some("Not Implemented")),
        (502, Some("Bad Gateway")),
        (503, Some("Service Unavailable")),
        (504, Some("Gateway Timeout")),
        (505, Some("HTTP Version Not Supported")),
        (506, Some("Variant Also Negotiates")),
        (507, Some("Insufficient Storage")),
        (508, Some("Loop Detected")),
        (510, Some("Not Extended")),
        (511, Some("Network Authentication Required")),
    ];

    for (code, expected_reason) in cases {
        assert_eq!(
            StatusCode::from_u16(code).canonical_reason(),
            expected_reason
        );
    }
}

#[test]
fn test_error_display_and_conversions_exhaustive() {
    let payload_err = RouterError::PayloadTooLarge {
        max_bytes: 1024,
        received_bytes: 2048,
    };
    assert!(payload_err.to_string().contains("413 Payload Too Large"));

    let sig_err = RouterError::InvalidSignature;
    assert!(sig_err.to_string().contains("403 Forbidden"));

    let params_err = RouterError::InvalidParams {
        message: "bad param".to_string(),
    };
    assert!(params_err.to_string().contains("400 Bad Request"));

    let deserial_err = RouterError::DeserializationError("bad format".to_string());
    assert!(
        deserial_err
            .to_string()
            .contains("422 Unprocessable Entity")
    );

    let conflict_err = RouterError::RouteConflict("conflict found".to_string());
    assert!(conflict_err.to_string().contains("Route collision"));

    let pipeline_err = RouterError::Pipeline("pipeline failed".to_string());
    assert!(
        pipeline_err
            .to_string()
            .contains("Pipeline execution error")
    );

    // Conversions
    let from_crypto: RouterError = gopon::Error::InvalidSignature.into();
    assert!(matches!(from_crypto, RouterError::InvalidSignature));

    let from_cookie: RouterError = chirkut::CookieError::InvalidName("bad".to_string()).into();
    assert!(matches!(from_cookie, RouterError::Internal(_)));

    let from_session: RouterError = session::SessionError::Serialization("err".to_string()).into();
    assert!(matches!(from_session, RouterError::Internal(_)));

    let from_pipe_next: RouterError = DhapError::<RouterError>::NextAlreadyCalled.into();
    assert!(matches!(from_pipe_next, RouterError::Pipeline(_)));

    let from_pipe_exec: RouterError = DhapError::Execution(RouterError::NotFound).into();
    assert!(matches!(from_pipe_exec, RouterError::NotFound));
}

#[test]
fn test_route_group_extended_methods() {
    let mut group = router::RouteGroup::new();
    group.prefix("/api");
    group.name("api.");

    group.get(
        "/get",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.post(
        "/post",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.put(
        "/put",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.delete(
        "/delete",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.patch(
        "/patch",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.head(
        "/head",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.options(
        "/options",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.any(
        "/any",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    group.get_fn("/get_fn", |ctx| {
        let _ = ctx;
        Box::pin(async move { "get_fn_resp" })
    });
    group.post_fn("/post_fn", |ctx| {
        let _ = ctx;
        Box::pin(async move { "post_fn_resp" })
    });

    let routes = group.into_routes();
    assert_eq!(routes.len(), 10);
    assert_eq!(routes[0].pattern(), "/api/get");
    assert_eq!(routes[0].route_name(), None);
}

struct EmptyResourceController;
impl ResourceController for EmptyResourceController {}

#[test]
fn test_resource_builder_extended_methods_and_defaults() {
    let controller = EmptyResourceController;
    let mut ctx = HttpContext::new(Request::builder().path("/test").build());

    // Calling unimplemented actions on default controller contract returns 405
    assert!(matches!(
        block_on(controller.index(&mut ctx)),
        Err(RouterError::MethodNotAllowed { .. })
    ));
    assert!(matches!(
        block_on(controller.create(&mut ctx)),
        Err(RouterError::MethodNotAllowed { .. })
    ));
    assert!(matches!(
        block_on(controller.store(&mut ctx)),
        Err(RouterError::MethodNotAllowed { .. })
    ));
    assert!(matches!(
        block_on(controller.show(&mut ctx)),
        Err(RouterError::MethodNotAllowed { .. })
    ));
    assert!(matches!(
        block_on(controller.edit(&mut ctx)),
        Err(RouterError::MethodNotAllowed { .. })
    ));
    assert!(matches!(
        block_on(controller.update(&mut ctx)),
        Err(RouterError::MethodNotAllowed { .. })
    ));
    assert!(matches!(
        block_on(controller.destroy(&mut ctx)),
        Err(RouterError::MethodNotAllowed { .. })
    ));

    // RouteResource builder methods
    let mut resource = RouteResource::new("users.comments", Arc::new(EmptyResourceController));
    resource.only(&[ResourceAction::Index, ResourceAction::Show]);
    assert_eq!(
        resource.actions(),
        &[ResourceAction::Index, ResourceAction::Show]
    );

    resource.except(&[ResourceAction::Show]);
    assert_eq!(resource.actions(), &[ResourceAction::Index]);

    resource.name("custom_alias");
    let routes = resource.into_routes();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].pattern(), "/users/:user_id/comments");
    assert_eq!(routes[0].route_name(), Some("custom_alias.index"));
}

#[test]
fn test_url_generator_extended_methods_and_errors() {
    let mut router = Router::new();
    router
        .get(
            "/posts/:id",
            from_handler(|ctx| {
                let _ = ctx;
                Box::pin(async move { Ok(()) })
            }),
        )
        .name("posts.show");

    let compiled = router.compile().expect("compile");
    let generator = compiled.url_generator();

    assert!(generator.signer().is_none());

    // Generating signed URL without signer returns error
    let signed_err = generator.signed_url_for("posts.show", &[("id", "1")]);
    assert!(matches!(signed_err, Err(RouterError::Internal(_))));

    let temp_signed_err = generator.temporary_signed_url_for("posts.show", &[("id", "1")], 60);
    assert!(matches!(temp_signed_err, Err(RouterError::Internal(_))));

    // Generator with base URL
    let with_base = generator.clone().with_base_url("https://example.com/");
    let full_url = with_base
        .url_for("posts.show", &[("id", "1")])
        .expect("url with base");
    assert_eq!(full_url, "https://example.com/posts/1");
}

#[test]
fn test_http_context_mutators_and_accessors() {
    let req = Request::builder().path("/ctx-test").build();
    let mut ctx = HttpContext::new(req);

    assert_eq!(ctx.request().path(), "/ctx-test");
    ctx.request_mut().set_path("/mutated");
    assert_eq!(ctx.request().path(), "/mutated");

    assert_eq!(ctx.response().status(), StatusCode::OK);
    ctx.response_mut().set_status(StatusCode::NO_CONTENT);
    assert_eq!(ctx.response().status(), StatusCode::NO_CONTENT);

    assert_eq!(ctx.extensions().get::<TestUserId>(), None);
    ctx.extensions_mut().insert(TestUserId(888));
    assert_eq!(ctx.extensions().get::<TestUserId>(), Some(&TestUserId(888)));

    assert!(ctx.session().is_none());
    let new_session =
        session::Session::new_fresh(session::SessionId::generate().expect("id"), 3600);
    ctx.set_session(new_session);
    assert!(ctx.session().is_some());
    let taken = ctx.take_session();
    assert!(taken.is_some());
    assert!(ctx.session().is_none());
}

#[test]
fn test_request_extended_methods_and_cookies() {
    let mut req = Request::builder()
        .method(Method::Get)
        .path("/test")
        .query("a=1&b=2")
        .header("X-Foo", "bar")
        .body(Bytes::from_static(b"sample bytes"))
        .build();

    assert_eq!(req.query(), Some("a=1&b=2"));
    assert_eq!(req.raw_bytes(), b"sample bytes");

    req.set_query(None);
    assert_eq!(req.query(), None);

    req.headers_mut().insert("X-Bar", "baz");
    assert_eq!(req.header("x-bar"), Some("baz"));

    req.set_body(Bytes::from_static(b"new bytes"));
    assert_eq!(req.raw_bytes(), b"new bytes");

    // RouteParams methods
    let mut params = router::RouteParams::new();
    assert!(params.is_empty());
    assert_eq!(params.len(), 0);
    params.insert("k", "v");
    assert!(!params.is_empty());
    assert_eq!(params.len(), 1);
    let pairs: Vec<_> = params.iter().collect();
    assert_eq!(pairs, vec![("k", "v")]);
}

#[test]
fn test_route_extended_accessors_and_constraints() {
    let mut route = router::Route::new(
        vec![Method::Get, Method::Head],
        "/items/:id",
        Arc::new(from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        })),
    );

    route.name("items.show");
    route.where_uuid("id");

    assert_eq!(route.pattern(), "/items/:id");
    assert_eq!(route.route_name(), Some("items.show"));
    assert_eq!(route.methods(), &[Method::Get, Method::Head]);
    assert_eq!(route.constraints().len(), 1);

    route.where_regex("id", r"^[a-z]+$").expect("valid regex");
    assert_eq!(route.constraints().len(), 1);

    route.prefix_name("v1.");
    assert_eq!(route.route_name(), Some("v1.items.show"));
}

#[test]
fn test_router_extended_closure_verbs() {
    let mut router = Router::new();
    router.get_fn("/get_fn", |ctx| {
        let _ = ctx;
        Box::pin(async move { "fn get" })
    });
    router.post_fn("/post_fn", |ctx| {
        let _ = ctx;
        Box::pin(async move { StatusCode::CREATED })
    });
    router.head(
        "/head_route",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );
    router.options(
        "/options_route",
        from_handler(|ctx| {
            let _ = ctx;
            Box::pin(async move { Ok(()) })
        }),
    );

    let compiled = router.compile().expect("compile extended verbs");

    let mut get_ctx = HttpContext::new(Request::builder().path("/get_fn").build());
    block_on(compiled.dispatch(&mut get_ctx)).expect("get_fn dispatch");
    assert_eq!(
        get_ctx.response.body().as_bytes(),
        Bytes::from_static(b"fn get")
    );

    let mut post_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Post)
            .path("/post_fn")
            .build(),
    );
    block_on(compiled.dispatch(&mut post_ctx)).expect("post_fn dispatch");
    assert_eq!(post_ctx.response.status(), StatusCode::CREATED);
}

// --- Property-Based Tests (proptest!) ---

proptest! {
    #[test]
    fn proptest_status_codes_roundtrip(code in 100u16..600u16) {
        let status = StatusCode::from_u16(code);
        prop_assert_eq!(status.as_u16(), code);

        let is_info = (100..200).contains(&code);
        let is_succ = (200..300).contains(&code);
        let is_redi = (300..400).contains(&code);
        let is_clie = (400..500).contains(&code);
        let is_serv = (500..600).contains(&code);

        prop_assert_eq!(status.is_informational(), is_info);
        prop_assert_eq!(status.is_success(), is_succ);
        prop_assert_eq!(status.is_redirection(), is_redi);
        prop_assert_eq!(status.is_client_error(), is_clie);
        prop_assert_eq!(status.is_server_error(), is_serv);
    }

    #[test]
    fn proptest_header_case_insensitivity(name in "[a-zA-Z0-9-]{1,30}", val in "[a-zA-Z0-9]{1,30}") {
        let lower = name.to_ascii_lowercase();
        let upper = name.to_ascii_uppercase();

        let header_name1 = HeaderName::new(&lower);
        let header_name2 = HeaderName::new(&upper);
        prop_assert_eq!(&header_name1, &header_name2);
        prop_assert_eq!(header_name1.as_str(), &lower);

        let mut map = HeaderMap::new();
        map.insert(header_name1, val.as_str());
        prop_assert_eq!(map.get(lower.as_str()), Some(val.as_str()));
        prop_assert_eq!(map.get(upper.as_str()), Some(val.as_str()));
    }
}

// --- Supplementary Targeted Coverage Tests ---

struct PassthroughTestMiddleware;

impl Middleware<HttpContext, Response, RouterError> for PassthroughTestMiddleware {
    fn handle<'a>(
        &'a self,
        ctx: &'a mut HttpContext,
        next: Next<HttpContext, Response, RouterError>,
    ) -> DhapBoxFuture<'a, Result<Response, DhapError<RouterError>>> {
        Box::pin(async move { next.run(ctx).await })
    }
}

struct MinimalResourceTestController;

impl ResourceController for MinimalResourceTestController {
    fn index<'a>(&self, ctx: &'a mut HttpContext) -> BoxFuture<'a, Result<(), RouterError>> {
        Box::pin(async move {
            ctx.response.text("resource index");
            Ok(())
        })
    }
}

#[test]
fn test_route_group_comprehensive_verbs_and_features() {
    let mut group = router::RouteGroup::default();
    group.prefix("/api/v1");
    group.name("api.");
    group.middleware_arc(Arc::new(PassthroughTestMiddleware));

    group.get(
        "/get",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group get");
                Ok(())
            })
        }),
    );
    group.post(
        "/post",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group post");
                Ok(())
            })
        }),
    );
    group.put(
        "/put",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group put");
                Ok(())
            })
        }),
    );
    group.patch(
        "/patch",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group patch");
                Ok(())
            })
        }),
    );
    group.delete(
        "/delete",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group delete");
                Ok(())
            })
        }),
    );
    group.head(
        "/head",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group head");
                Ok(())
            })
        }),
    );
    group.options(
        "/options",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group options");
                Ok(())
            })
        }),
    );
    group.any(
        "/any",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("group any");
                Ok(())
            })
        }),
    );
    group.get_fn("/get_fn", |ctx| {
        Box::pin(async move {
            let _ = ctx;
            "group get_fn"
        })
    });
    group.post_fn("/post_fn", |ctx| {
        Box::pin(async move {
            let _ = ctx;
            "group post_fn"
        })
    });
    group.resource("items", MinimalResourceTestController);
    group.resource_with("photos", MinimalResourceTestController, |resource| {
        resource.only(&[ResourceAction::Index]);
    });
    group.group(|nested| {
        nested.get(
            "/inner",
            from_handler(|ctx| {
                Box::pin(async move {
                    ctx.response.text("group nested");
                    Ok(())
                })
            }),
        );
    });
    group.group_prefixed("/deep", |nested| {
        nested.get(
            "/inner",
            from_handler(|ctx| {
                Box::pin(async move {
                    ctx.response.text("group deep nested");
                    Ok(())
                })
            }),
        );
    });
    group.add_route(router::Route::new(
        vec![Method::Get],
        "/manual",
        Arc::new(from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("manual route");
                Ok(())
            })
        })),
    ));

    let mut router = Router::new();
    router.group(|builder| {
        for route in group.into_routes() {
            builder.add_route(route);
        }
    });

    let compiled = router.compile().expect("compilation succeeded");

    let verbs = [
        (Method::Get, "/api/v1/get", "group get"),
        (Method::Post, "/api/v1/post", "group post"),
        (Method::Put, "/api/v1/put", "group put"),
        (Method::Patch, "/api/v1/patch", "group patch"),
        (Method::Delete, "/api/v1/delete", "group delete"),
        (Method::Options, "/api/v1/options", "group options"),
        (Method::Get, "/api/v1/any", "group any"),
        (Method::Post, "/api/v1/any", "group any"),
        (Method::Get, "/api/v1/get_fn", "group get_fn"),
        (Method::Post, "/api/v1/post_fn", "group post_fn"),
        (Method::Get, "/api/v1/items", "resource index"),
        (Method::Get, "/api/v1/photos", "resource index"),
        (Method::Get, "/api/v1/inner", "group nested"),
        (Method::Get, "/api/v1/deep/inner", "group deep nested"),
        (Method::Get, "/api/v1/manual", "manual route"),
    ];

    for (method, path, expected_body) in verbs {
        let mut ctx = HttpContext::new(Request::builder().method(method).path(path).build());
        block_on(compiled.dispatch(&mut ctx))
            .unwrap_or_else(|error| panic!("failed for {path}: {error:?}"));
        assert_eq!(
            ctx.response.body().as_bytes(),
            Bytes::copy_from_slice(expected_body.as_bytes())
        );
    }

    let mut head_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Head)
            .path("/api/v1/head")
            .build(),
    );
    block_on(compiled.dispatch(&mut head_ctx)).expect("head dispatch");
    assert_eq!(head_ctx.response.body().as_bytes(), Bytes::new());
}

#[test]
fn test_route_builder_and_constraints_deep() {
    let mut route = router::Route::new(
        vec![Method::Get],
        "test/:alpha/:alnum/:slug",
        Arc::new(from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("matched");
                Ok(())
            })
        })),
    );

    route.where_alpha("alpha");
    route.where_alphanumeric("alnum");
    route.where_slug("slug");
    route.middleware_arc(Arc::new(PassthroughTestMiddleware));

    let regex_err = route.where_regex("param", "[invalid(");
    assert!(regex_err.is_err());

    route.prefix_pattern("");
    assert_eq!(route.pattern(), "/test/:alpha/:alnum/:slug");
    route.prefix_pattern("/base");
    assert_eq!(route.pattern(), "/base/test/:alpha/:alnum/:slug");

    assert_eq!(route.route_name(), None);
    route.prefix_name("prefix.");
    assert_eq!(route.route_name(), None);
    route.name("route.name");
    assert_eq!(route.route_name(), Some("route.name"));
    route.prefix_name("prefix.");
    assert_eq!(route.route_name(), Some("prefix.route.name"));

    assert_eq!(route.methods(), &[Method::Get]);
    assert_eq!(route.middlewares().len(), 1);
    assert_eq!(route.constraints().len(), 3);
}

#[test]
fn test_router_builder_and_extensions_deep() {
    let mut router = Router::default();
    router.config_mut().max_body_size = 10_000;
    assert_eq!(router.config_mut().max_body_size, 10_000);

    router.middleware_arc(Arc::new(PassthroughTestMiddleware));

    let mut sub = Router::new();
    sub.get(
        "/dashboard",
        from_handler(|ctx| {
            Box::pin(async move {
                ctx.response.text("dashboard");
                Ok(())
            })
        }),
    )
    .name("dashboard");

    router.nest_as("/admin", "admin.", sub);
    router.redirect("/old_home", "/new_home", StatusCode::MOVED_PERMANENTLY);
    router.resource_with("articles", MinimalResourceTestController, |res| {
        res.except(&[ResourceAction::Destroy]);
    });

    let compiled = router.compile().expect("compilation");

    let mut admin_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Get)
            .path("/admin/dashboard")
            .build(),
    );
    block_on(compiled.dispatch(&mut admin_ctx)).expect("admin dispatch");
    assert_eq!(
        admin_ctx.response.body().as_bytes(),
        Bytes::from_static(b"dashboard")
    );
    assert_eq!(admin_ctx.matched_route_name(), Some("admin.dashboard"));

    let mut redirect_ctx = HttpContext::new(
        Request::builder()
            .method(Method::Get)
            .path("/old_home")
            .build(),
    );
    block_on(compiled.dispatch(&mut redirect_ctx)).expect("redirect dispatch");
    assert_eq!(
        redirect_ctx.response.status(),
        StatusCode::MOVED_PERMANENTLY
    );
    assert_eq!(
        redirect_ctx.response.headers().get(HeaderName::LOCATION),
        Some("/new_home")
    );

    let mut ext = router::Extensions::default();
    assert!(ext.is_empty());
    assert_eq!(ext.len(), 0);

    let first_insert = ext.insert("value1".to_string());
    assert_eq!(first_insert, None);
    assert!(!ext.is_empty());
    assert_eq!(ext.len(), 1);
    assert!(ext.contains::<String>());
    assert!(!ext.contains::<u64>());

    let second_insert = ext.insert("value2".to_string());
    assert_eq!(second_insert, Some("value1".to_string()));
    assert_eq!(ext.get::<String>(), Some(&"value2".to_string()));

    if let Some(val_mut) = ext.get_mut::<String>() {
        val_mut.push_str("_modified");
    }
    assert_eq!(ext.get::<String>(), Some(&"value2_modified".to_string()));

    let debug_repr = format!("{ext:?}");
    assert!(debug_repr.contains("Extensions"));

    let removed = ext.remove::<String>();
    assert_eq!(removed, Some("value2_modified".to_string()));
    assert!(ext.is_empty());

    ext.insert(12345u32);
    assert_eq!(ext.len(), 1);
    ext.clear();
    assert!(ext.is_empty());
}

#[test]
fn test_request_and_response_edge_cases_and_accessors() {
    let mut req = Request::builder()
        .method(Method::Post)
        .path("/hello/world?query=val")
        .query("custom=123")
        .param("key", "val")
        .body("request body payload")
        .client_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
        .trust_proxy(false)
        .header("X-Test", "Sample")
        .header(HeaderName::HOST, "example.test")
        .build();

    assert_eq!(req.intended_method(), &Method::Post);
    req.set_method(Method::Put);
    assert_eq!(req.method(), &Method::Put);

    req.set_path("/new_path");
    assert_eq!(req.path(), "/new_path");

    req.set_query(Some("foo=bar".to_string()));
    assert_eq!(req.query(), Some("foo=bar"));

    req.headers_mut().insert("X-Added", "AddedVal");
    assert_eq!(req.header("x-added"), Some("AddedVal"));

    req.params_mut().insert("num", "42");
    req.params_mut().insert("bad_num", "not_a_num");
    assert_eq!(req.param("num"), Some("42"));
    assert_eq!(req.param_parsed::<u32>("num"), Some(Ok(42)));
    assert!(req.param_parsed::<u32>("bad_num").unwrap().is_err());
    assert_eq!(req.param_parsed::<u32>("non_existent"), None);

    req.set_body(Bytes::from_static(b"updated payload"));
    assert_eq!(req.raw_bytes(), b"updated payload");

    assert_eq!(req.host(), Some("example.test"));
    assert!(!req.is_secure());
    assert!(!req.is_json());
    assert!(!req.accepts("image/png"));

    let full_with_query = req.full_uri();
    assert_eq!(full_with_query, "/new_path?foo=bar");

    req.set_query(None);
    let full_without_query = req.full_uri();
    assert_eq!(full_without_query, "/new_path");

    let debug_req = format!("{req:?}");
    assert!(debug_req.contains("Request"));

    let invalid_utf8_req = Request::builder()
        .body(Bytes::from_static(&[0xFF, 0xFE, 0xFD]))
        .build();
    assert!(invalid_utf8_req.text().is_err());
    assert!(invalid_utf8_req.json::<serde_json::Value>().is_err());
    assert!(invalid_utf8_req.form::<serde_json::Value>().is_err());

    let mut route_params = router::RouteParams::new();
    assert!(route_params.is_empty());
    assert_eq!(route_params.len(), 0);
    route_params.insert("k", "v");
    assert!(!route_params.is_empty());
    assert_eq!(route_params.len(), 1);
    let collected_params = route_params.iter().collect::<Vec<_>>();
    assert_eq!(collected_params, vec![("k", "v")]);

    let mut res = Response::default();
    assert_eq!(res.status(), StatusCode::OK);
    res.set_status(StatusCode::ACCEPTED);
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    let res_with_status = Response::ok().with_status(StatusCode::NO_CONTENT);
    assert_eq!(res_with_status.status(), StatusCode::NO_CONTENT);

    let res_with_body = Response::ok().with_body(ResponseBody::Text("custom body".to_string()));
    assert_eq!(
        res_with_body.body().as_bytes(),
        Bytes::from_static(b"custom body")
    );

    let _ = res.body_mut();
    res.headers_mut().insert("X-Custom-Header", "Value");
    assert_eq!(res.headers().get("x-custom-header"), Some("Value"));

    let empty_body = ResponseBody::default();
    assert!(empty_body.is_empty());
    assert_eq!(empty_body.len(), 0);

    let bytes_body = ResponseBody::Bytes(Bytes::from_static(b"sample bytes"));
    assert!(!bytes_body.is_empty());
    assert_eq!(bytes_body.len(), 12);
    assert_eq!(bytes_body.as_bytes(), Bytes::from_static(b"sample bytes"));

    let see_other_redir = Redirect::see_other("/other");
    assert_eq!(see_other_redir.status(), StatusCode::SEE_OTHER);
    assert_eq!(see_other_redir.destination(), "/other");

    let perm_redir = Redirect::permanent("/perm");
    assert_eq!(perm_redir.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(perm_redir.destination(), "/perm");

    let temp_redir = Redirect::temporary("/temp");
    assert_eq!(temp_redir.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(temp_redir.destination(), "/temp");
}

#[test]
fn test_header_map_and_name_full_coverage() {
    let name_from_str = "x-my-header".parse::<HeaderName>().expect("infallible");
    assert_eq!(name_from_str.as_str(), "x-my-header");
    assert_eq!(format!("{name_from_str}"), "x-my-header");

    let static_name = HeaderName::from_static("x-static");
    assert_eq!(static_name.as_str(), "x-static");

    let val_from_str = HeaderValue::from("val1");
    let val_from_string = HeaderValue::from(String::from("val2"));
    assert_eq!(val_from_str.as_str(), "val1");
    assert_eq!(format!("{val_from_string}"), "val2");

    let mut map: HeaderMap = vec![("X-Item", "one"), ("X-Item", "two"), ("X-Other", "three")]
        .into_iter()
        .collect();

    assert_eq!(map.len(), 2);
    assert!(!map.is_empty());
    assert!(map.contains_key("x-item"));
    assert!(map.contains_key("x-other"));
    assert!(!map.contains_key("x-absent"));

    let removed = map.remove("x-other");
    assert!(removed.is_some());
    assert_eq!(map.len(), 1);

    let entries = map.iter().collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);

    map.clear();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
}

#[test]
fn test_error_conversions_and_display_complete() {
    use dhap::Error as DhapError;
    use poth::Error as PothError;

    let missing_param = PothError::MissingParameter {
        name: "id".to_string(),
        route: "/users/:id".to_string(),
    };
    let err_missing: RouterError = missing_param.into();
    assert!(matches!(err_missing, RouterError::InvalidParams { .. }));

    let conflicting = PothError::ConflictingRoute {
        existing: "/a".to_string(),
        new_pattern: "/b".to_string(),
        reason: "overlap".to_string(),
    };
    let err_conflict: RouterError = conflicting.into();
    assert!(matches!(err_conflict, RouterError::RouteConflict(_)));

    let invalid_pattern = PothError::InvalidPattern {
        pattern: "/*".to_string(),
        reason: "wildcard error".to_string(),
    };
    let err_pattern: RouterError = invalid_pattern.into();
    assert!(matches!(err_pattern, RouterError::RouteConflict(_)));

    let decode_error = PothError::DecodeError {
        param: "p".to_string(),
        value: "%invalid".to_string(),
    };
    let err_decode: RouterError = decode_error.into();
    assert!(matches!(err_decode, RouterError::InvalidParams { .. }));

    let pipeline_err: RouterError = DhapError::NextAlreadyCalled.into();
    assert!(matches!(pipeline_err, RouterError::Pipeline(_)));

    let crypto_sig_err: RouterError = gopon::Error::InvalidSignature.into();
    assert_eq!(crypto_sig_err, RouterError::InvalidSignature);

    let crypto_exp_err: RouterError = gopon::Error::ExpiredSignature {
        expires: 10,
        current: 20,
    }
    .into();
    assert_eq!(crypto_exp_err, RouterError::InvalidSignature);

    let crypto_other_err: RouterError = gopon::Error::AuthenticationFailed.into();
    assert!(matches!(crypto_other_err, RouterError::Internal(_)));

    let cookie_err: RouterError = chirkut::CookieError::InvalidName("bad name".to_string()).into();
    assert!(matches!(cookie_err, RouterError::Internal(_)));

    let qs_err: RouterError = khonj::Error::Custom("qs error".to_string()).into();
    assert!(matches!(qs_err, RouterError::DeserializationError(_)));

    let session_err: RouterError = session::SessionError::Store("storage err".to_string()).into();
    assert!(matches!(session_err, RouterError::Internal(_)));

    let errors_to_test = [
        RouterError::NotFound,
        RouterError::MethodNotAllowed {
            allowed: vec![Method::Get, Method::Post],
        },
        RouterError::PayloadTooLarge {
            max_bytes: 100,
            received_bytes: 200,
        },
        RouterError::InvalidSignature,
        RouterError::InvalidParams {
            message: "param failure".to_string(),
        },
        RouterError::DeserializationError("malformed".to_string()),
        RouterError::RouteConflict("conflict".to_string()),
        RouterError::Internal("crash".to_string()),
        RouterError::Pipeline("pipeline failed".to_string()),
    ];

    for err in errors_to_test {
        let display_str = format!("{err}");
        assert!(!display_str.is_empty());
        assert!(std::error::Error::source(&err).is_none());

        let mut plain_ctx = HttpContext::new(Request::builder().build());
        err.clone().apply(&mut plain_ctx).expect("apply plain text");
        assert!(!plain_ctx.response.body().as_bytes().is_empty());

        let mut json_ctx = HttpContext::new(
            Request::builder()
                .header(HeaderName::ACCEPT, "application/json")
                .build(),
        );
        err.apply(&mut json_ctx).expect("apply json");
        assert!(
            json_ctx
                .response
                .body()
                .as_bytes()
                .starts_with(b"{\"error\":")
        );
    }
}

#[test]
fn test_url_generator_without_signer_and_accessors() {
    let mut matcher = poth::MethodMatcher::new();
    matcher
        .insert_named("home", Method::Get, "/home", ())
        .expect("insert named");
    let generator =
        router::UrlGenerator::new(Arc::new(matcher)).with_base_url("https://example.com/");
    assert!(generator.signer().is_none());

    let absolute_url = generator.url_for("home", &[]).expect("url_for");
    assert_eq!(absolute_url, "https://example.com/home");

    let signed_res = generator.signed_url_for("home", &[]);
    assert!(matches!(signed_res, Err(RouterError::Internal(_))));

    let temp_res = generator.temporary_signed_url_for("home", &[], 60);
    assert!(matches!(temp_res, Err(RouterError::Internal(_))));
}

#[test]
fn test_context_accessors_and_matched_route() {
    let mut ctx = HttpContext::new(Request::builder().build());
    assert_eq!(ctx.matched_route_name(), None);
    ctx.set_matched_route_name(Some("test.route".to_string()));
    assert_eq!(ctx.matched_route_name(), Some("test.route"));

    let req_ref = ctx.request();
    let _ = req_ref;
    let req_mut = ctx.request_mut();
    let _ = req_mut;
    let res_ref = ctx.response();
    let _ = res_ref;
    let res_mut = ctx.response_mut();
    let _ = res_mut;
    let ext_ref = ctx.extensions();
    let _ = ext_ref;
    let ext_mut = ctx.extensions_mut();
    let _ = ext_mut;
    let jar = ctx.cookie_jar();
    let _ = jar;
}

#[test]
fn test_status_code_edge_cases() {
    assert_eq!(StatusCode::default(), StatusCode::OK);

    let unknown_code = StatusCode::from_u16(999);
    assert_eq!(unknown_code.canonical_reason(), None);
    assert_eq!(format!("{unknown_code}"), "999");
}

struct HeaderAppendingMiddleware;
impl Middleware<HttpContext, Response, RouterError> for HeaderAppendingMiddleware {
    fn handle<'a>(
        &'a self,
        ctx: &'a mut HttpContext,
        next: Next<HttpContext, Response, RouterError>,
    ) -> DhapBoxFuture<'a, Result<Response, DhapError<RouterError>>> {
        Box::pin(async move {
            ctx.response_mut()
                .set_header(HeaderName::from("x-global"), "applied");
            next.run(ctx).await
        })
    }
}

struct PassthroughMiddleware;
impl Middleware<HttpContext, Response, RouterError> for PassthroughMiddleware {
    fn handle<'a>(
        &'a self,
        ctx: &'a mut HttpContext,
        next: Next<HttpContext, Response, RouterError>,
    ) -> DhapBoxFuture<'a, Result<Response, DhapError<RouterError>>> {
        Box::pin(async move { next.run(ctx).await })
    }
}

fn dummy_terminal_handler(ctx: &mut HttpContext) -> BoxFuture<'_, Result<(), RouterError>> {
    Box::pin(async move {
        ctx.response.text("ok");
        Ok(())
    })
}

#[test]
fn test_compiled_options_auto_negotiation_and_fallback_with_middleware() {
    let mut router = Router::new();
    router.middleware(HeaderAppendingMiddleware);
    router.get_fn("/data", |ctx: &mut HttpContext| {
        Box::pin(async move {
            let _ = ctx;
            "data response"
        })
    });
    router.fallback(from_handler(|ctx: &mut HttpContext| {
        Box::pin(async move {
            ctx.response.set_status(StatusCode::NOT_FOUND);
            ctx.response.text("custom fallback");
            Ok(())
        })
    }));

    let compiled = router.compile().expect("compile router");

    // Test automatic OPTIONS method negotiation when GET is defined
    let options_req = Request::builder()
        .method(Method::Options)
        .path("/data")
        .build();
    let mut options_ctx = HttpContext::new(options_req);
    block_on(compiled.dispatch(&mut options_ctx)).expect("handle options");
    assert_eq!(options_ctx.response.status(), StatusCode::NO_CONTENT);
    let allow_header = options_ctx
        .response
        .headers()
        .get(HeaderName::ALLOW)
        .expect("allow header");
    assert!(allow_header.contains("GET"));
    assert!(allow_header.contains("HEAD"));
    assert!(allow_header.contains("OPTIONS"));

    // Test fallback with global middleware
    let not_found_req = Request::builder()
        .method(Method::Get)
        .path("/missing-route")
        .build();
    let mut not_found_ctx = HttpContext::new(not_found_req);
    block_on(compiled.dispatch(&mut not_found_ctx)).expect("handle fallback");
    assert_eq!(not_found_ctx.response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        not_found_ctx
            .response
            .headers()
            .get(HeaderName::from("x-global")),
        Some("applied")
    );
}

#[test]
fn test_router_with_config_and_base_url() {
    let config = router::RouterConfig {
        base_url: Some("https://druto.dev".to_string()),
        allow_method_spoofing: false,
        trust_proxy: false,
        max_body_size: 1024,
        url_signer: None,
    };
    let mut router = Router::with_config(config);
    router.get_fn("/docs", |ctx: &mut HttpContext| {
        Box::pin(async move {
            let _ = ctx;
            "docs"
        })
    });
    assert_eq!(router.config_mut().max_body_size, 1024);

    let compiled = router.compile().expect("compile");
    let url = compiled.url_generator().url_for("docs", &[]);
    assert!(url.is_err());
}

#[test]
fn test_context_and_request_extended_coverage() {
    let mut ctx = HttpContext::new(Request::builder().build());
    let cookie_jar_mut_ref = ctx.cookie_jar_mut();
    cookie_jar_mut_ref.set(chirkut::Cookie::new("session_token", "abc").expect("cookie"));
    assert_eq!(
        ctx.cookie_jar().get("session_token").expect("get cookie"),
        Some("abc".to_string())
    );

    let key = Key::generate().expect("generate key");
    let ring = KeyRing::new(key.clone());
    let signer = UrlSigner::new(key);

    let signed_url = signer.sign_temporary("/profile?tab=security", 10_000);
    let mut parts = signed_url.splitn(2, '?');
    let path = parts.next().unwrap();
    let query = parts.next().map(ToString::to_string);

    let mut req = Request::builder()
        .path(path)
        .keyring(ring)
        .trust_proxy(true)
        .header(HeaderName::X_FORWARDED_FOR, "   , 1.2.3.4")
        .header(HeaderName::X_REAL_IP, "   ")
        .build();
    req.set_query(query);

    let headers_mut_ref = req.headers_mut();
    headers_mut_ref.insert("x-custom", "val");
    assert_eq!(req.header("x-custom"), Some("val"));

    let params_mut_ref = req.params_mut();
    assert!(params_mut_ref.is_empty());
    assert!(req.params().is_empty());

    assert!(req.has_valid_signature_until(&signer, 100));
}

struct DefaultDummyController;
impl ResourceController for DefaultDummyController {}

#[test]
fn test_resource_controller_defaults_and_action_str() {
    let controller = DefaultDummyController;
    let mut ctx = HttpContext::new(Request::builder().build());

    let index_res = block_on(controller.index(&mut ctx));
    assert!(matches!(
        index_res,
        Err(RouterError::MethodNotAllowed { .. })
    ));

    let create_res = block_on(controller.create(&mut ctx));
    assert!(matches!(
        create_res,
        Err(RouterError::MethodNotAllowed { .. })
    ));

    let store_res = block_on(controller.store(&mut ctx));
    assert!(matches!(
        store_res,
        Err(RouterError::MethodNotAllowed { .. })
    ));

    let show_res = block_on(controller.show(&mut ctx));
    assert!(matches!(
        show_res,
        Err(RouterError::MethodNotAllowed { .. })
    ));

    let edit_res = block_on(controller.edit(&mut ctx));
    assert!(matches!(
        edit_res,
        Err(RouterError::MethodNotAllowed { .. })
    ));

    let update_res = block_on(controller.update(&mut ctx));
    assert!(matches!(
        update_res,
        Err(RouterError::MethodNotAllowed { .. })
    ));

    let destroy_res = block_on(controller.destroy(&mut ctx));
    assert!(matches!(
        destroy_res,
        Err(RouterError::MethodNotAllowed { .. })
    ));

    assert_eq!(ResourceAction::Index.as_str(), "index");
    assert_eq!(ResourceAction::Create.as_str(), "create");
    assert_eq!(ResourceAction::Store.as_str(), "store");
    assert_eq!(ResourceAction::Show.as_str(), "show");
    assert_eq!(ResourceAction::Edit.as_str(), "edit");
    assert_eq!(ResourceAction::Update.as_str(), "update");
    assert_eq!(ResourceAction::Destroy.as_str(), "destroy");

    let mut resource = RouteResource::new("posts", Arc::new(controller));
    resource.middleware(PassthroughMiddleware);
    let routes = resource.into_routes();
    assert_eq!(routes.len(), 7);
}

#[test]
fn test_route_and_group_verbs_and_patterns() {
    let mut route =
        router::Route::new(vec![Method::Get], "items", Arc::new(dummy_terminal_handler));
    route.prefix_pattern("");
    assert_eq!(route.pattern(), "/items");
    route.prefix_pattern("/api");
    assert_eq!(route.pattern(), "/api/items");

    let mut group = router::RouteGroup::new();
    group.any("/any", dummy_terminal_handler);
    group.options("/opts", dummy_terminal_handler);
    group.head("/head", dummy_terminal_handler);
    group.put("/put", dummy_terminal_handler);
    group.patch("/patch", dummy_terminal_handler);
    group.delete("/delete", dummy_terminal_handler);
    group.middleware(PassthroughMiddleware);

    let routes = group.into_routes();
    assert_eq!(routes.len(), 6);
}

#[test]
fn test_response_status_and_url_generator_exhaustive() {
    let created = Response::created();
    assert_eq!(created.status(), StatusCode::CREATED);

    let no_content = Response::no_content();
    assert_eq!(no_content.status(), StatusCode::NO_CONTENT);

    let not_found = Response::not_found();
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);

    let empty_text = ResponseBody::Text(String::new());
    assert!(empty_text.is_empty());
    let non_empty_text = ResponseBody::Text("hello".to_string());
    assert!(!non_empty_text.is_empty());

    assert_eq!(StatusCode::from(200u16), StatusCode::OK);
    assert_eq!(
        HeaderName::from("custom-name".to_string()).as_str(),
        "custom-name"
    );

    let mut matcher = poth::MethodMatcher::new();
    matcher
        .insert_named("home", Method::Get, "/home", ())
        .expect("insert named");
    let key = Key::generate().expect("generate key");
    let signer = UrlSigner::new(key);
    let generator = router::UrlGenerator::new(Arc::new(matcher)).with_signer(signer);
    let temp_url = generator
        .temporary_signed_url_for("home", &[], 3600)
        .expect("temp url");
    assert!(temp_url.contains("signature="));

    let method_err = RouterError::from(poth::Error::MethodNotAllowed {
        method: Method::Post,
        path: "/test".to_string(),
        allowed: vec![Method::Get],
    });
    assert!(matches!(method_err, RouterError::MethodNotAllowed { .. }));

    let constraint_err = RouterError::from(poth::Error::ConstraintFailed {
        param: "id".to_string(),
        value: "abc".to_string(),
        constraint: r"\d+".to_string(),
    });
    assert!(matches!(constraint_err, RouterError::InvalidParams { .. }));

    // Test request.headers() accessor
    let req = Request::builder()
        .header(HeaderName::CONTENT_TYPE, "application/json")
        .build();
    assert_eq!(req.headers().len(), 1);

    // Test route.prefix_pattern with empty or root prefix
    let mut route_empty_prefix = router::Route::new(
        vec![Method::Get],
        "dashboard",
        Arc::new(dummy_terminal_handler),
    );
    route_empty_prefix.prefix_pattern("/");
    assert_eq!(route_empty_prefix.pattern(), "/dashboard");
}
