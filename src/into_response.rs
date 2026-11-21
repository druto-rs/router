//! Conversions from handler return types into HTTP response representations.

use serde_json::Value as JsonValue;

use crate::{
    context::HttpContext,
    error::RouterError,
    header::HeaderName,
    response::{Redirect, Response},
    status::StatusCode,
};

/// Conversion contract for types that can be applied to outgoing HTTP responses.
pub trait IntoResponse: Send + 'static {
    /// Applies this value to the response stored on [`HttpContext`].
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError>;
}

impl IntoResponse for () {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        let _ = ctx;
        Ok(())
    }
}

impl IntoResponse for Response {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        ctx.response.set_status(self.status());
        for (name, values) in self.headers().iter() {
            for value in values {
                ctx.response.append_header(name.clone(), value.clone());
            }
        }
        ctx.response.set_body(self.body().clone());
        Ok(())
    }
}

impl IntoResponse for &'static str {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        ctx.response.text(self);
        Ok(())
    }
}

impl IntoResponse for String {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        ctx.response.text(self);
        Ok(())
    }
}

impl IntoResponse for Redirect {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        let destination = self.destination().to_string();
        let status = self.status();
        ctx.response.redirect(destination, Some(status));
        Ok(())
    }
}

impl IntoResponse for JsonValue {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        ctx.response.json(&self)
    }
}

impl IntoResponse for StatusCode {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        ctx.response.set_status(self);
        Ok(())
    }
}

impl<T: IntoResponse> IntoResponse for (StatusCode, T) {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        ctx.response.set_status(self.0);
        self.1.apply(ctx)
    }
}

impl<T, E> IntoResponse for Result<T, E>
where
    T: IntoResponse,
    E: IntoResponse,
{
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        match self {
            Ok(value) => value.apply(ctx),
            Err(error) => error.apply(ctx),
        }
    }
}

impl IntoResponse for RouterError {
    fn apply(self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        let (status, message, allowed_header) = match &self {
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string(), None),
            Self::MethodNotAllowed { allowed } => {
                let allowed_str = allowed
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    StatusCode::METHOD_NOT_ALLOWED,
                    self.to_string(),
                    Some(allowed_str),
                )
            }
            Self::PayloadTooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, self.to_string(), None),
            Self::InvalidSignature => (StatusCode::FORBIDDEN, self.to_string(), None),
            Self::InvalidParams { .. } => (StatusCode::BAD_REQUEST, self.to_string(), None),
            Self::DeserializationError(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, self.to_string(), None)
            }
            Self::RouteConflict(_) | Self::Internal(_) | Self::Pipeline(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string(), None)
            }
        };

        ctx.response.set_status(status);

        if let Some(allow) = allowed_header {
            ctx.response.set_header(HeaderName::ALLOW, allow);
        }

        if ctx.request.wants_json() {
            let json_body = serde_json::json!({
                "error": {
                    "status": status.as_u16(),
                    "message": message,
                }
            });
            ctx.response.json(&json_body)?;
        } else {
            ctx.response.text(message);
        }

        Ok(())
    }
}
