//! HTTP response status codes conforming to RFC 9110.

use std::fmt;

/// An HTTP status code representing the result of an HTTP request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusCode(u16);

impl StatusCode {
    /// 100 Continue
    pub const CONTINUE: Self = Self(100);
    /// 101 Switching Protocols
    pub const SWITCHING_PROTOCOLS: Self = Self(101);

    /// 200 OK
    pub const OK: Self = Self(200);
    /// 201 Created
    pub const CREATED: Self = Self(201);
    /// 202 Accepted
    pub const ACCEPTED: Self = Self(202);
    /// 204 No Content
    pub const NO_CONTENT: Self = Self(204);
    /// 205 Reset Content
    pub const RESET_CONTENT: Self = Self(205);
    /// 206 Partial Content
    pub const PARTIAL_CONTENT: Self = Self(206);

    /// 301 Moved Permanently
    pub const MOVED_PERMANENTLY: Self = Self(301);
    /// 302 Found
    pub const FOUND: Self = Self(302);
    /// 303 See Other
    pub const SEE_OTHER: Self = Self(303);
    /// 304 Not Modified
    pub const NOT_MODIFIED: Self = Self(304);
    /// 307 Temporary Redirect
    pub const TEMPORARY_REDIRECT: Self = Self(307);
    /// 308 Permanent Redirect
    pub const PERMANENT_REDIRECT: Self = Self(308);

    /// 400 Bad Request
    pub const BAD_REQUEST: Self = Self(400);
    /// 401 Unauthorized
    pub const UNAUTHORIZED: Self = Self(401);
    /// 403 Forbidden
    pub const FORBIDDEN: Self = Self(403);
    /// 404 Not Found
    pub const NOT_FOUND: Self = Self(404);
    /// 405 Method Not Allowed
    pub const METHOD_NOT_ALLOWED: Self = Self(405);
    /// 406 Not Acceptable
    pub const NOT_ACCEPTABLE: Self = Self(406);
    /// 408 Request Timeout
    pub const REQUEST_TIMEOUT: Self = Self(408);
    /// 409 Conflict
    pub const CONFLICT: Self = Self(409);
    /// 410 Gone
    pub const GONE: Self = Self(410);
    /// 413 Payload Too Large
    pub const PAYLOAD_TOO_LARGE: Self = Self(413);
    /// 414 URI Too Long
    pub const URI_TOO_LONG: Self = Self(414);
    /// 415 Unsupported Media Type
    pub const UNSUPPORTED_MEDIA_TYPE: Self = Self(415);
    /// 422 Unprocessable Entity
    pub const UNPROCESSABLE_ENTITY: Self = Self(422);
    /// 429 Too Many Requests
    pub const TOO_MANY_REQUESTS: Self = Self(429);

    /// 500 Internal Server Error
    pub const INTERNAL_SERVER_ERROR: Self = Self(500);
    /// 501 Not Implemented
    pub const NOT_IMPLEMENTED: Self = Self(501);
    /// 502 Bad Gateway
    pub const BAD_GATEWAY: Self = Self(502);
    /// 503 Service Unavailable
    pub const SERVICE_UNAVAILABLE: Self = Self(503);
    /// 504 Gateway Timeout
    pub const GATEWAY_TIMEOUT: Self = Self(504);

    /// Creates a status code from an integer value.
    #[must_use]
    pub const fn from_u16(code: u16) -> Self {
        Self(code)
    }

    /// Returns the integer numerical representation of the status code.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Returns the canonical RFC reason phrase for standard HTTP status codes.
    #[must_use]
    pub const fn canonical_reason(self) -> Option<&'static str> {
        match self.0 {
            100 => Some("Continue"),
            101 => Some("Switching Protocols"),
            102 => Some("Processing"),
            103 => Some("Early Hints"),
            200 => Some("OK"),
            201 => Some("Created"),
            202 => Some("Accepted"),
            203 => Some("Non-Authoritative Information"),
            204 => Some("No Content"),
            205 => Some("Reset Content"),
            206 => Some("Partial Content"),
            207 => Some("Multi-Status"),
            208 => Some("Already Reported"),
            226 => Some("IM Used"),
            300 => Some("Multiple Choices"),
            301 => Some("Moved Permanently"),
            302 => Some("Found"),
            303 => Some("See Other"),
            304 => Some("Not Modified"),
            305 => Some("Use Proxy"),
            307 => Some("Temporary Redirect"),
            308 => Some("Permanent Redirect"),
            400 => Some("Bad Request"),
            401 => Some("Unauthorized"),
            402 => Some("Payment Required"),
            403 => Some("Forbidden"),
            404 => Some("Not Found"),
            405 => Some("Method Not Allowed"),
            406 => Some("Not Acceptable"),
            407 => Some("Proxy Authentication Required"),
            408 => Some("Request Timeout"),
            409 => Some("Conflict"),
            410 => Some("Gone"),
            411 => Some("Length Required"),
            412 => Some("Precondition Failed"),
            413 => Some("Payload Too Large"),
            414 => Some("URI Too Long"),
            415 => Some("Unsupported Media Type"),
            416 => Some("Range Not Satisfiable"),
            417 => Some("Expectation Failed"),
            418 => Some("I'm a teapot"),
            421 => Some("Misdirected Request"),
            422 => Some("Unprocessable Entity"),
            423 => Some("Locked"),
            424 => Some("Failed Dependency"),
            425 => Some("Too Early"),
            426 => Some("Upgrade Required"),
            428 => Some("Precondition Required"),
            429 => Some("Too Many Requests"),
            431 => Some("Request Header Fields Too Large"),
            451 => Some("Unavailable For Legal Reasons"),
            500 => Some("Internal Server Error"),
            501 => Some("Not Implemented"),
            502 => Some("Bad Gateway"),
            503 => Some("Service Unavailable"),
            504 => Some("Gateway Timeout"),
            505 => Some("HTTP Version Not Supported"),
            506 => Some("Variant Also Negotiates"),
            507 => Some("Insufficient Storage"),
            508 => Some("Loop Detected"),
            510 => Some("Not Extended"),
            511 => Some("Network Authentication Required"),
            _ => None,
        }
    }

    /// Checks if the status code is informational (100-199).
    #[must_use]
    pub const fn is_informational(self) -> bool {
        self.0 >= 100 && self.0 < 200
    }

    /// Checks if the status code is successful (200-299).
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    /// Checks if the status code is a redirection (300-399).
    #[must_use]
    pub const fn is_redirection(self) -> bool {
        self.0 >= 300 && self.0 < 400
    }

    /// Checks if the status code is a client error (400-499).
    #[must_use]
    pub const fn is_client_error(self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    /// Checks if the status code is a server error (500-599).
    #[must_use]
    pub const fn is_server_error(self) -> bool {
        self.0 >= 500 && self.0 < 600
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.canonical_reason() {
            Some(reason) => write!(formatter, "{} {}", self.0, reason),
            None => write!(formatter, "{}", self.0),
        }
    }
}

impl From<u16> for StatusCode {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl Default for StatusCode {
    fn default() -> Self {
        Self::OK
    }
}
