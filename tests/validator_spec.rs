//! Tests for HTTP router validation integration, HttpContextValidateExt, and 422 content negotiation.

use druto_router as router;
use druto_router::{
    context::HttpContext, header::HeaderName, into_response::IntoResponse, prelude::*,
    request::Request, status::StatusCode,
};
use jachai::prelude::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct CreateUserInput {
    name: String,
    email: String,
}

#[test]
fn test_validate_body_success() {
    let req = Request::builder()
        .path("/users")
        .json(&json!({
            "name": "Druto Developer",
            "email": "dev@druto.dev"
        }))
        .unwrap()
        .build();

    let mut ctx = HttpContext::new(req);

    let user_schema = object()
        .field("name", string().min_len(3))
        .field("email", string().email());

    let validated: CreateUserInput = ctx
        .validate_body(&user_schema.decode_into::<CreateUserInput>())
        .unwrap();

    assert_eq!(validated.name, "Druto Developer");
    assert_eq!(validated.email, "dev@druto.dev");
}

#[test]
fn test_validate_query_and_params() {
    let req = Request::builder()
        .path("/articles/42")
        .param("id", "42")
        .query("filter=active&limit=10")
        .build();

    let ctx = HttpContext::new(req);

    let params_schema = object().field("id", integer().positive());
    let validated_params = ctx.validate_params(&params_schema).unwrap();
    assert_eq!(
        validated_params.get("id").and_then(|v| v.as_i64()),
        Some(42)
    );

    let query_schema = object()
        .field("filter", string())
        .field("limit", integer().positive());
    let validated_query = ctx.validate_query(&query_schema).unwrap();
    assert_eq!(
        validated_query.get("filter").and_then(|v| v.as_str()),
        Some("active")
    );
    assert_eq!(
        validated_query.get("limit").and_then(|v| v.as_i64()),
        Some(10)
    );
}

#[test]
fn test_validate_request_all_components() {
    let req = Request::builder()
        .path("/posts/99")
        .param("post_id", "99")
        .query("sort=asc")
        .json(&json!({ "title": "Framework Design" }))
        .unwrap()
        .build();

    let mut ctx = HttpContext::new(req);

    let composite_schema = request_schema()
        .params(object().field("post_id", integer().positive()))
        .query(object().field("sort", string()))
        .body(object().field("title", string().min_len(3)));

    let validated = ctx.validate_request(&composite_schema).unwrap();
    assert_eq!(validated.params.unwrap()["post_id"], 99);
    assert_eq!(validated.query.unwrap()["sort"], "asc");
    assert_eq!(validated.body.unwrap()["title"], "Framework Design");
}

#[tokio::test]
async fn test_validate_body_and_request_async() {
    let req = Request::builder()
        .path("/async_users")
        .json(&json!({
            "name": "Async Developer",
            "email": "async@druto.dev"
        }))
        .unwrap()
        .build();

    let mut ctx = HttpContext::new(req);

    let user_schema = object()
        .field("name", string().min_len(3))
        .field("email", string().email());

    let validated: CreateUserInput = ctx
        .validate_body_async(&user_schema.decode_into::<CreateUserInput>())
        .await
        .unwrap();

    assert_eq!(validated.name, "Async Developer");
    assert_eq!(validated.email, "async@druto.dev");
}

#[test]
fn test_422_into_response_simple_json_negotiation() {
    let req = Request::builder()
        .path("/api/resource")
        .header(HeaderName::ACCEPT, "application/json")
        .build();

    let mut ctx = HttpContext::new(req);

    let mut err = ValidationError::new();
    err.push(ValidationIssue::new(
        "email",
        "email",
        "The email field must be a valid email address.",
    ));

    // Apply error into response
    err.apply(&mut ctx).unwrap();

    assert_eq!(ctx.response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        ctx.response
            .headers()
            .get(HeaderName::CONTENT_TYPE)
            .map(|h| h.to_string()),
        Some("application/json".to_string())
    );

    let json_body: serde_json::Value = match ctx.response.body() {
        router::response::ResponseBody::Json(serialized) => {
            serde_json::from_str(serialized).unwrap()
        }
        other => panic!("Expected JSON response body, got {other:?}"),
    };
    assert!(json_body.get("errors").is_some());
    assert_eq!(json_body["errors"][0]["field"], "email");
    assert_eq!(json_body["errors"][0]["rule"], "email");
}

#[test]
fn test_422_into_response_rfc9457_negotiation() {
    let req = Request::builder()
        .path("/api/resource")
        .header(HeaderName::ACCEPT, "application/problem+json")
        .build();

    let mut ctx = HttpContext::new(req);

    let mut err = ValidationError::new();
    err.push(ValidationIssue::new(
        "title",
        "min_len",
        "The title field must be at least 5 characters.",
    ));

    // Apply error into response
    err.apply(&mut ctx).unwrap();

    assert_eq!(ctx.response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        ctx.response
            .headers()
            .get(HeaderName::CONTENT_TYPE)
            .map(|h| h.to_string()),
        Some("application/problem+json".to_string())
    );

    let json_body: serde_json::Value = match ctx.response.body() {
        router::response::ResponseBody::Json(serialized) => {
            serde_json::from_str(serialized).unwrap()
        }
        other => panic!("Expected JSON response body, got {other:?}"),
    };
    assert_eq!(json_body["type"], "https://tools.ietf.org/html/rfc9457");
    assert_eq!(json_body["title"], "Unprocessable Entity");
    assert_eq!(json_body["status"], 422);
    assert_eq!(json_body["instance"], "/api/resource");
    assert!(json_body.get("invalid_params").is_some());
    assert_eq!(json_body["invalid_params"][0]["name"], "title");
    assert_eq!(
        json_body["invalid_params"][0]["reason"],
        "The title field must be at least 5 characters."
    );
}
