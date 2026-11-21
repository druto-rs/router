//! Strongly typed error taxonomy for routing, dispatch, parameter extraction, and execution.

use std::fmt;

use dhap::Error as DhapError;
use khonj::Error as QsError;
use poth::{Error as PothError, Method};

/// Errors produced during route registration, path compilation, request dispatch, or handler execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterError {
    /// No matching route was found for the requested URI path.
    NotFound,
    /// A route matched the path, but does not allow the requested HTTP method.
    MethodNotAllowed {
        /// The methods allowed for this path.
        allowed: Vec<Method>,
    },
    /// The incoming request body exceeded the maximum configured byte ceiling.
    PayloadTooLarge {
        /// The maximum allowed payload size in bytes.
        max_bytes: usize,
        /// The actual number of payload bytes received.
        received_bytes: usize,
    },
    /// A cryptographic URL signature was missing, expired, or invalid.
    InvalidSignature,
    /// Route parameter validation failed.
    InvalidParams {
        /// Detailed explanation of why parameters failed validation.
        message: String,
    },
    /// Request payload deserialization failed (e.g. malformed JSON or query parameters).
    DeserializationError(String),
    /// A route collides with an existing route definition during compilation.
    RouteConflict(String),
    /// An internal execution failure occurred.
    Internal(String),
    /// An error occurred during middleware pipeline dispatch.
    Pipeline(String),
}

impl fmt::Display for RouterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("404 Not Found"),
            Self::MethodNotAllowed { allowed } => {
                let methods = allowed
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "405 Method Not Allowed (Allowed: {methods})")
            }
            Self::PayloadTooLarge {
                max_bytes,
                received_bytes,
            } => {
                write!(
                    formatter,
                    "413 Payload Too Large (Received {received_bytes} bytes, limit is {max_bytes} bytes)"
                )
            }
            Self::InvalidSignature => {
                formatter.write_str("403 Forbidden: Invalid or expired URL signature")
            }
            Self::InvalidParams { message } => write!(formatter, "400 Bad Request: {message}"),
            Self::DeserializationError(message) => {
                write!(formatter, "422 Unprocessable Entity: {message}")
            }
            Self::RouteConflict(message) => write!(formatter, "Route collision: {message}"),
            Self::Internal(message) => write!(formatter, "500 Internal Server Error: {message}"),
            Self::Pipeline(message) => write!(formatter, "Pipeline execution error: {message}"),
        }
    }
}

impl std::error::Error for RouterError {}

impl From<PothError> for RouterError {
    fn from(error: PothError) -> Self {
        match error {
            PothError::NotFound { .. } | PothError::RouteNotFound { .. } => Self::NotFound,
            PothError::MethodNotAllowed { allowed, .. } => Self::MethodNotAllowed { allowed },
            PothError::ConstraintFailed {
                param, constraint, ..
            } => Self::InvalidParams {
                message: format!("Parameter '{param}' failed constraint '{constraint}'"),
            },
            PothError::MissingParameter { name, route } => Self::InvalidParams {
                message: format!("Missing required parameter '{name}' for route '{route}'"),
            },
            PothError::ConflictingRoute {
                existing,
                new_pattern,
                reason,
            } => Self::RouteConflict(format!(
                "Pattern '{new_pattern}' conflicts with existing '{existing}': {reason}"
            )),
            PothError::InvalidPattern { pattern, reason } => {
                Self::RouteConflict(format!("Invalid pattern '{pattern}': {reason}"))
            }
            PothError::DecodeError { param, value } => Self::InvalidParams {
                message: format!(
                    "Failed to percent-decode parameter '{param}' with value '{value}'"
                ),
            },
        }
    }
}

impl From<DhapError<Self>> for RouterError {
    fn from(error: DhapError<Self>) -> Self {
        match error {
            DhapError::NextAlreadyCalled => {
                Self::Pipeline("Middleware step next was invoked more than once".to_string())
            }
            DhapError::Execution(inner) => inner,
        }
    }
}

impl From<serde_json::Error> for RouterError {
    fn from(error: serde_json::Error) -> Self {
        Self::DeserializationError(error.to_string())
    }
}

impl From<gopon::Error> for RouterError {
    fn from(error: gopon::Error) -> Self {
        match error {
            gopon::Error::InvalidSignature | gopon::Error::ExpiredSignature { .. } => {
                Self::InvalidSignature
            }
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<chirkut::CookieError> for RouterError {
    fn from(error: chirkut::CookieError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<QsError> for RouterError {
    fn from(error: QsError) -> Self {
        Self::DeserializationError(error.to_string())
    }
}

impl From<session::SessionError> for RouterError {
    fn from(error: session::SessionError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<bojha::BodyParserError> for RouterError {
    fn from(error: bojha::BodyParserError) -> Self {
        match error {
            bojha::BodyParserError::PayloadTooLarge { limit, received } => Self::PayloadTooLarge {
                max_bytes: limit,
                received_bytes: received,
            },
            bojha::BodyParserError::JsonError(msg)
            | bojha::BodyParserError::FormError(msg)
            | bojha::BodyParserError::Utf8Error(msg)
            | bojha::BodyParserError::MalformedMultipart(msg) => Self::DeserializationError(msg),
            other => Self::Internal(other.to_string()),
        }
    }
}

impl From<lagam::LimiterError> for RouterError {
    fn from(error: lagam::LimiterError) -> Self {
        match error {
            lagam::LimiterError::Store(message)
            | lagam::LimiterError::StoreUnavailable(message) => {
                Self::Internal(format!("Limiter error: {message}"))
            }
            lagam::LimiterError::Config(message) => {
                Self::Internal(format!("Limiter configuration error: {message}"))
            }
            lagam::LimiterError::RateLimitExceeded(_) => {
                Self::Internal("429 Too Many Requests".to_string())
            }
        }
    }
}
