//! Request-scoped execution context passed through middleware to route handlers.

use chirkut::CookieJar;
use session::{Session, SessionContext};

use crate::{extensions::Extensions, header::HeaderName, request::Request, response::Response};

/// Request execution context encapsulating the incoming request, outgoing response,
/// session state, and type-safe request extensions.
pub struct HttpContext {
    /// The incoming HTTP request.
    pub request: Request,
    /// The outgoing HTTP response.
    pub response: Response,
    /// Active session state, if session middleware is active.
    pub session: Option<Session>,
    /// Type-safe request-local storage container.
    pub extensions: Extensions,
    /// The name of the route that matched the request, if named.
    pub matched_route_name: Option<String>,
}

impl HttpContext {
    /// Creates a new `HttpContext` wrapping an incoming [`Request`].
    #[must_use]
    pub fn new(request: Request) -> Self {
        Self {
            request,
            response: Response::ok(),
            session: None,
            extensions: Extensions::new(),
            matched_route_name: None,
        }
    }

    /// Returns an immutable reference to the incoming request.
    #[must_use]
    pub fn request(&self) -> &Request {
        &self.request
    }

    /// Returns a mutable reference to the incoming request.
    pub fn request_mut(&mut self) -> &mut Request {
        &mut self.request
    }

    /// Returns an immutable reference to the outgoing response.
    #[must_use]
    pub fn response(&self) -> &Response {
        &self.response
    }

    /// Returns a mutable reference to the outgoing response.
    pub fn response_mut(&mut self) -> &mut Response {
        &mut self.response
    }

    /// Returns an immutable reference to the request cookie jar.
    #[must_use]
    pub fn cookie_jar(&self) -> &CookieJar {
        self.request.cookie_jar()
    }

    /// Returns a mutable reference to the request cookie jar.
    pub fn cookie_jar_mut(&mut self) -> &mut CookieJar {
        self.request.cookie_jar_mut()
    }

    /// Returns an immutable reference to the type-safe extensions container.
    #[must_use]
    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    /// Returns a mutable reference to the type-safe extensions container.
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    /// Returns the matched route name if one was resolved.
    #[must_use]
    pub fn matched_route_name(&self) -> Option<&str> {
        self.matched_route_name.as_deref()
    }

    /// Sets the matched route name.
    pub fn set_matched_route_name(&mut self, name: Option<String>) {
        self.matched_route_name = name;
    }

    /// Renders any staged response cookies from the cookie jar into `Set-Cookie` headers on the response.
    pub fn flush_cookies_to_response(&mut self) {
        for header_value in self.request.cookie_jar().render_set_cookie_headers() {
            self.response
                .append_header(HeaderName::SET_COOKIE, header_value);
        }
    }
}

impl SessionContext for HttpContext {
    fn cookie_jar_mut(&mut self) -> &mut CookieJar {
        self.request.cookie_jar_mut()
    }

    fn cookie_jar(&self) -> &CookieJar {
        self.request.cookie_jar()
    }

    fn set_session(&mut self, session: Session) {
        self.session = Some(session);
    }

    fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    fn take_session(&mut self) -> Option<Session> {
        self.session.take()
    }
}
