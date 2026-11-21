//! HTTP router validation integration, context extraction traits, and automatic 422 error response handling.

use futures_util::future::BoxFuture;
use jachai::{
    RequestParts, RequestSchema, Schema, ValidatedRequest, ValidationContext, ValidationError,
};
use serde_json::Value;

use crate::{
    RouterError, context::HttpContext, header::HeaderName, into_response::IntoResponse,
    request::Request, status::StatusCode,
};

impl IntoResponse for ValidationError {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        ctx.response.set_status(StatusCode::UNPROCESSABLE_ENTITY);

        let accepts_problem = ctx
            .request
            .header(HeaderName::ACCEPT)
            .map(|val| val.contains("application/problem+json"))
            .unwrap_or(false);

        if accepts_problem {
            let problem = self.to_problem_details(Some(ctx.request.path()));
            ctx.response.json(&problem)?;
            ctx.response
                .set_header(HeaderName::CONTENT_TYPE, "application/problem+json");
        } else {
            let simple = self.to_json_value();
            ctx.response.json(&simple)?;
            ctx.response
                .set_header(HeaderName::CONTENT_TYPE, "application/json");
        }

        Ok(())
    }
}

impl From<ValidationError> for RouterError {
    fn from(error: ValidationError) -> Self {
        RouterError::DeserializationError(error.to_string())
    }
}

/// Extension trait providing declarative validation methods on [`HttpContext`].
pub trait HttpContextValidateExt {
    /// Validates the incoming JSON request body against a schema.
    fn validate_body<S, T>(&mut self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>;

    /// Validates parsed query string parameters against a schema.
    fn validate_query<S, T>(&self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>;

    /// Validates route parameters against a schema.
    fn validate_params<S, T>(&self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>;

    /// Validates incoming HTTP headers against a schema.
    fn validate_headers<S, T>(&self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>;

    /// Validates all components of the request using a composite [`RequestSchema`].
    fn validate_request(
        &mut self,
        schema: &RequestSchema,
    ) -> Result<ValidatedRequest, ValidationError>;

    /// Asynchronously validates the request body against a schema.
    fn validate_body_async<'a, S, T>(
        &'a mut self,
        schema: &'a S,
    ) -> BoxFuture<'a, Result<T, ValidationError>>
    where
        S: Schema<T> + 'a,
        T: 'a;

    /// Asynchronously validates all request components using a composite [`RequestSchema`].
    fn validate_request_async<'a>(
        &'a mut self,
        schema: &'a RequestSchema,
    ) -> BoxFuture<'a, Result<ValidatedRequest, ValidationError>>;
}

impl HttpContextValidateExt for HttpContext {
    fn validate_body<S, T>(&mut self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>,
    {
        let body_val = extract_body_json(&self.request)?;
        let mut vctx = ValidationContext::new();
        populate_context_from_request(&self.request, &mut vctx);
        schema.validate_with_context(&body_val, &vctx)
    }

    fn validate_query<S, T>(&self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>,
    {
        let query_val = extract_query_json(&self.request);
        let mut vctx = ValidationContext::new();
        populate_context_from_request(&self.request, &mut vctx);
        schema.validate_with_context(&query_val, &vctx)
    }

    fn validate_params<S, T>(&self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>,
    {
        let params_val = extract_params_json(&self.request);
        let mut vctx = ValidationContext::new();
        populate_context_from_request(&self.request, &mut vctx);
        schema.validate_with_context(&params_val, &vctx)
    }

    fn validate_headers<S, T>(&self, schema: &S) -> Result<T, ValidationError>
    where
        S: Schema<T>,
    {
        let headers_val = extract_headers_json(&self.request);
        let mut vctx = ValidationContext::new();
        populate_context_from_request(&self.request, &mut vctx);
        schema.validate_with_context(&headers_val, &vctx)
    }

    fn validate_request(
        &mut self,
        schema: &RequestSchema,
    ) -> Result<ValidatedRequest, ValidationError> {
        let parts = extract_request_parts(&self.request)?;
        let mut vctx = ValidationContext::new();
        populate_context_from_request(&self.request, &mut vctx);
        schema.validate_with_context(&parts, &vctx)
    }

    fn validate_body_async<'a, S, T>(
        &'a mut self,
        schema: &'a S,
    ) -> BoxFuture<'a, Result<T, ValidationError>>
    where
        S: Schema<T> + 'a,
        T: 'a,
    {
        Box::pin(async move {
            let body_val = extract_body_json(&self.request)?;
            let mut vctx = ValidationContext::new();
            populate_context_from_request(&self.request, &mut vctx);
            schema.validate_async_with_context(&body_val, &vctx).await
        })
    }

    fn validate_request_async<'a>(
        &'a mut self,
        schema: &'a RequestSchema,
    ) -> BoxFuture<'a, Result<ValidatedRequest, ValidationError>> {
        Box::pin(async move {
            let parts = extract_request_parts(&self.request)?;
            let mut vctx = ValidationContext::new();
            populate_context_from_request(&self.request, &mut vctx);
            schema.validate_async_with_context(&parts, &vctx).await
        })
    }
}

fn extract_body_json(request: &Request) -> Result<Value, ValidationError> {
    if request.body().is_empty() {
        return Ok(Value::Null);
    }

    request
        .json::<Value>()
        .map_err(|err| ValidationError::single("body", "malformed_json", err.to_string()))
}

fn extract_query_json(request: &Request) -> Value {
    let query_str = match request.query() {
        Some(s) if !s.is_empty() => s,
        _ => return Value::Object(serde_json::Map::new()),
    };

    if let Ok(tree) = khonj::parse(query_str) {
        return khonj_value_to_json(&tree);
    }

    let mut map = serde_json::Map::new();
    for pair in query_str.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(k.to_string(), Value::String(v.to_string()));
        } else if !pair.is_empty() {
            map.insert(pair.to_string(), Value::Null);
        }
    }
    Value::Object(map)
}

fn khonj_value_to_json(val: &khonj::Value) -> Value {
    match val {
        khonj::Value::Null => Value::Null,
        khonj::Value::Scalar(s) => Value::String(s.clone()),
        khonj::Value::Array(items) => Value::Array(items.iter().map(khonj_value_to_json).collect()),
        khonj::Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), khonj_value_to_json(v));
            }
            Value::Object(obj)
        }
    }
}

fn extract_params_json(request: &Request) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in request.params().iter() {
        map.insert(k.to_string(), Value::String(v.to_string()));
    }
    Value::Object(map)
}

fn extract_headers_json(request: &Request) -> Value {
    let mut map = serde_json::Map::new();
    for (name, values) in request.headers().iter() {
        if let Some(first_val) = values.first() {
            map.insert(
                name.as_str().to_string(),
                Value::String(first_val.to_string()),
            );
        }
    }
    Value::Object(map)
}

fn extract_cookies_json(request: &Request) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in request.cookie_jar().incoming() {
        map.insert(name.to_string(), Value::String(value.to_string()));
    }
    Value::Object(map)
}

fn extract_request_parts(request: &Request) -> Result<RequestParts, ValidationError> {
    let body = extract_body_json(request)?;
    let query = extract_query_json(request);
    let params = extract_params_json(request);
    let headers = extract_headers_json(request);
    let cookies = extract_cookies_json(request);

    Ok(RequestParts {
        params,
        query,
        headers,
        cookies,
        body,
    })
}

fn populate_context_from_request(request: &Request, context: &mut ValidationContext) {
    if let Some(ip) = request.ip() {
        context.insert(ip);
    }
}
