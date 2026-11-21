//! Outgoing HTTP response representation, header manipulation, and payload bodies.

use bytes::Bytes;
use serde::Serialize;

use crate::{
    error::RouterError,
    header::{HeaderMap, HeaderName, HeaderValue},
    status::StatusCode,
};

/// Encapsulated response body payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ResponseBody {
    /// Zero-length response payload.
    #[default]
    Empty,
    /// Raw binary bytes payload.
    Bytes(Bytes),
    /// Plain text payload with UTF-8 character encoding.
    Text(String),
    /// HTML document payload with UTF-8 character encoding.
    Html(String),
    /// JSON serialized payload.
    Json(String),
}

impl ResponseBody {
    /// Returns the body payload rendered as raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> Bytes {
        match self {
            Self::Empty => Bytes::new(),
            Self::Bytes(bytes) => bytes.clone(),
            Self::Text(text) | Self::Html(text) | Self::Json(text) => {
                Bytes::copy_from_slice(text.as_bytes())
            }
        }
    }

    /// Returns `true` if the body is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Bytes(bytes) => bytes.is_empty(),
            Self::Text(text) | Self::Html(text) | Self::Json(text) => text.is_empty(),
        }
    }

    /// Returns the length in bytes of the payload body.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Bytes(bytes) => bytes.len(),
            Self::Text(text) | Self::Html(text) | Self::Json(text) => text.len(),
        }
    }
}

/// Outgoing HTTP response representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: ResponseBody,
}

impl Response {
    /// Creates a new response with the given status code and empty body.
    #[must_use]
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: ResponseBody::Empty,
        }
    }

    /// Creates a 200 OK response with an empty body.
    #[must_use]
    pub fn ok() -> Self {
        Self::new(StatusCode::OK)
    }

    /// Creates a 201 Created response.
    #[must_use]
    pub fn created() -> Self {
        Self::new(StatusCode::CREATED)
    }

    /// Creates a 204 No Content response.
    #[must_use]
    pub fn no_content() -> Self {
        Self::new(StatusCode::NO_CONTENT)
    }

    /// Creates a 404 Not Found response.
    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND)
    }

    /// Returns the response status code.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Sets the response status code.
    pub fn set_status(&mut self, status: StatusCode) {
        self.status = status;
    }

    /// Fluent builder method to set the response status code.
    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Returns an immutable reference to the response headers map.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns a mutable reference to the response headers map.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Sets a header on the response.
    pub fn set_header(&mut self, name: impl Into<HeaderName>, value: impl Into<HeaderValue>) {
        self.headers.insert(name, value);
    }

    /// Appends a header to the response.
    pub fn append_header(&mut self, name: impl Into<HeaderName>, value: impl Into<HeaderValue>) {
        self.headers.append(name, value);
    }

    /// Fluent builder method to insert a header.
    #[must_use]
    pub fn header(mut self, name: impl Into<HeaderName>, value: impl Into<HeaderValue>) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Returns an immutable reference to the response body.
    #[must_use]
    pub fn body(&self) -> &ResponseBody {
        &self.body
    }

    /// Returns a mutable reference to the response body.
    pub fn body_mut(&mut self) -> &mut ResponseBody {
        &mut self.body
    }

    /// Sets the response body.
    pub fn set_body(&mut self, body: ResponseBody) {
        self.body = body;
    }

    /// Fluent builder method to set the response body.
    #[must_use]
    pub fn with_body(mut self, body: ResponseBody) -> Self {
        self.body = body;
        self
    }

    /// Serializes a value to JSON, sets the `Content-Type` header, and populates the body.
    pub fn json<T: Serialize>(&mut self, value: &T) -> Result<(), RouterError> {
        let serialized = serde_json::to_string(value)?;
        self.headers
            .insert(HeaderName::CONTENT_TYPE, "application/json; charset=utf-8");
        self.body = ResponseBody::Json(serialized);
        Ok(())
    }

    /// Sets HTML content on the response and sets the `Content-Type` header to `text/html; charset=utf-8`.
    pub fn html(&mut self, content: impl Into<String>) {
        self.headers
            .insert(HeaderName::CONTENT_TYPE, "text/html; charset=utf-8");
        self.body = ResponseBody::Html(content.into());
    }

    /// Sets plain text on the response and sets the `Content-Type` header to `text/plain; charset=utf-8`.
    pub fn text(&mut self, content: impl Into<String>) {
        self.headers
            .insert(HeaderName::CONTENT_TYPE, "text/plain; charset=utf-8");
        self.body = ResponseBody::Text(content.into());
    }

    /// Configures the response as a redirect to the specified destination path.
    pub fn redirect(&mut self, destination: impl Into<String>, status: Option<StatusCode>) {
        let code = status.unwrap_or(StatusCode::FOUND);
        self.status = code;
        self.headers
            .insert(HeaderName::LOCATION, destination.into());
        self.body = ResponseBody::Empty;
    }
}

impl Default for Response {
    fn default() -> Self {
        Self::ok()
    }
}

/// Fluent helper for constructing redirection responses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    destination: String,
    status: StatusCode,
}

impl Redirect {
    /// Creates a 302 Found temporary redirect.
    #[must_use]
    pub fn to(destination: impl Into<String>) -> Self {
        Self {
            destination: destination.into(),
            status: StatusCode::FOUND,
        }
    }

    /// Creates a 301 Moved Permanently redirect.
    #[must_use]
    pub fn permanent(destination: impl Into<String>) -> Self {
        Self {
            destination: destination.into(),
            status: StatusCode::MOVED_PERMANENTLY,
        }
    }

    /// Creates a 303 See Other redirect.
    #[must_use]
    pub fn see_other(destination: impl Into<String>) -> Self {
        Self {
            destination: destination.into(),
            status: StatusCode::SEE_OTHER,
        }
    }

    /// Creates a 307 Temporary Redirect preserving HTTP method.
    #[must_use]
    pub fn temporary(destination: impl Into<String>) -> Self {
        Self {
            destination: destination.into(),
            status: StatusCode::TEMPORARY_REDIRECT,
        }
    }

    /// Returns the target destination URL.
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// Returns the redirect HTTP status code.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }
}
