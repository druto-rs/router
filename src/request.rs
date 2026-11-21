//! Strongly typed HTTP request representation, payload inspection, and content negotiation.

use std::{fmt, net::IpAddr, str::FromStr};

use bytes::Bytes;
use chirkut::CookieJar;
use gopon::{KeyRing, UrlSigner};
use khonj::from_str as qs_from_str;
use poth::Method;
use serde::de::DeserializeOwned;

use crate::{
    error::RouterError,
    header::{HeaderMap, HeaderName},
};

/// Route path parameters extracted during pattern matching.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteParams {
    entries: Vec<(String, String)>,
}

impl RouteParams {
    /// Creates an empty parameter collection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Inserts a key-value parameter pair.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.push((key.into(), value.into()));
    }

    /// Looks up a parameter by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, val)| val.as_str())
    }

    /// Parses a parameter into a typed value implementing [`FromStr`].
    pub fn parse<T: FromStr>(&self, name: &str) -> Option<Result<T, T::Err>> {
        self.get(name).map(str::parse::<T>)
    }

    /// Returns an iterator over all parameter key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(key, val)| (key.as_str(), val.as_str()))
    }

    /// Returns the number of extracted parameters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no parameters were extracted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Incoming HTTP request representation.
#[derive(Clone)]
pub struct Request {
    method: Method,
    intended_method: Method,
    path: String,
    query: Option<String>,
    headers: HeaderMap,
    params: RouteParams,
    body: Bytes,
    cookie_jar: CookieJar,
    client_ip: Option<IpAddr>,
    trust_proxy: bool,
}

impl Request {
    /// Creates a new `RequestBuilder` to fluently assemble an HTTP request.
    #[must_use]
    pub fn builder() -> RequestBuilder {
        RequestBuilder::new()
    }

    /// Returns the effective HTTP method (accounting for method spoofing).
    #[must_use]
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Sets the effective HTTP method.
    pub fn set_method(&mut self, method: Method) {
        self.method = method;
    }

    /// Returns the raw underlying transport HTTP method.
    #[must_use]
    pub fn intended_method(&self) -> &Method {
        &self.intended_method
    }

    /// Returns the URI path component.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Sets the request URI path.
    pub fn set_path(&mut self, path: impl Into<String>) {
        self.path = path.into();
    }

    /// Returns the raw query string if present, excluding the leading `?`.
    #[must_use]
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// Sets the raw query string.
    pub fn set_query(&mut self, query: Option<String>) {
        self.query = query;
    }

    /// Returns an immutable reference to the request headers.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns a mutable reference to the request headers.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Looks up a header value by name.
    #[must_use]
    pub fn header(&self, name: impl Into<HeaderName>) -> Option<&str> {
        self.headers.get(name)
    }

    /// Returns an immutable reference to the extracted route parameters.
    #[must_use]
    pub fn params(&self) -> &RouteParams {
        &self.params
    }

    /// Returns a mutable reference to the extracted route parameters.
    pub fn params_mut(&mut self) -> &mut RouteParams {
        &mut self.params
    }

    /// Looks up an extracted path parameter by name.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.get(name)
    }

    /// Parses an extracted path parameter into a typed value.
    pub fn param_parsed<T: FromStr>(&self, name: &str) -> Option<Result<T, T::Err>> {
        self.params.parse::<T>(name)
    }

    /// Returns an immutable reference to the buffered request body bytes.
    #[must_use]
    pub fn body(&self) -> &Bytes {
        &self.body
    }

    /// Sets the buffered request body bytes.
    pub fn set_body(&mut self, body: Bytes) {
        self.body = body;
    }

    /// Returns the raw body bytes.
    #[must_use]
    pub fn raw_bytes(&self) -> &[u8] {
        &self.body
    }

    /// Returns an immutable reference to the request cookie jar.
    #[must_use]
    pub fn cookie_jar(&self) -> &CookieJar {
        &self.cookie_jar
    }

    /// Returns a mutable reference to the request cookie jar.
    pub fn cookie_jar_mut(&mut self) -> &mut CookieJar {
        &mut self.cookie_jar
    }

    /// Deserializes the query string into a typed struct using nested bracket syntax parsing.
    pub fn query_parsed<T: DeserializeOwned>(&self) -> Result<T, RouterError> {
        let raw = self.query.as_deref().unwrap_or("");
        qs_from_str::<T>(raw).map_err(Into::into)
    }

    /// Deserializes the request body as JSON into a typed struct.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, RouterError> {
        serde_json::from_slice::<T>(&self.body).map_err(Into::into)
    }

    /// Returns the request body as a UTF-8 string slice.
    pub fn text(&self) -> Result<&str, RouterError> {
        std::str::from_utf8(&self.body).map_err(|error| {
            RouterError::DeserializationError(format!("Malformed UTF-8 body: {error}"))
        })
    }

    /// Deserializes form urlencoded payloads into a typed struct.
    pub fn form<T: DeserializeOwned>(&self) -> Result<T, RouterError> {
        let body_str = self.text()?;
        qs_from_str::<T>(body_str).map_err(Into::into)
    }

    /// Evaluates client IP address, honoring proxy headers if `trust_proxy` is enabled.
    #[must_use]
    pub fn ip(&self) -> Option<String> {
        if self.trust_proxy {
            if let Some(forwarded) = self.headers.get(HeaderName::FORWARDED) {
                for part in forwarded.split(';') {
                    let trimmed = part.trim();
                    if let Some(for_part) = trimmed.strip_prefix("for=") {
                        let ip_str = for_part.trim_matches('"').trim();
                        if !ip_str.is_empty() {
                            return Some(ip_str.to_string());
                        }
                    }
                }
            }
            if let Some(forwarded_for) = self.headers.get(HeaderName::X_FORWARDED_FOR)
                && let Some(first_ip) = forwarded_for.split(',').next()
            {
                let trimmed = first_ip.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            if let Some(real_ip) = self.headers.get(HeaderName::X_REAL_IP) {
                let trimmed = real_ip.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        self.client_ip.map(|ip| ip.to_string())
    }

    /// Returns `true` if the request was made over HTTPS.
    #[must_use]
    pub fn is_secure(&self) -> bool {
        if self.trust_proxy
            && let Some(proto) = self.headers.get(HeaderName::X_FORWARDED_PROTO)
        {
            return proto.eq_ignore_ascii_case("https");
        }
        false
    }

    /// Returns the resolved Host of the request.
    #[must_use]
    pub fn host(&self) -> Option<&str> {
        if self.trust_proxy
            && let Some(forwarded_host) = self.headers.get(HeaderName::X_FORWARDED_HOST)
        {
            return Some(forwarded_host);
        }
        self.headers.get(HeaderName::HOST)
    }

    /// Content negotiation: checks if the `Accept` header matches the given MIME type.
    #[must_use]
    pub fn accepts(&self, mime: &str) -> bool {
        self.headers
            .get(HeaderName::ACCEPT)
            .is_some_and(|accept| accept.contains(mime) || accept.contains("*/*"))
    }

    /// Returns `true` if the request `Content-Type` indicates JSON payload.
    #[must_use]
    pub fn is_json(&self) -> bool {
        self.headers
            .get(HeaderName::CONTENT_TYPE)
            .is_some_and(|content_type| content_type.contains("application/json"))
    }

    /// Returns `true` if the client requested a JSON response via `Accept`.
    #[must_use]
    pub fn wants_json(&self) -> bool {
        self.accepts("application/json")
    }

    /// Verifies the cryptographic HMAC signature of the URL.
    #[must_use]
    pub fn has_valid_signature(&self, signer: &UrlSigner) -> bool {
        let full_uri = self.full_uri();
        signer.verify(&full_uri).is_ok()
    }

    /// Verifies the cryptographic HMAC signature and checks timestamp expiration against `current_unix_time`.
    #[must_use]
    pub fn has_valid_signature_until(&self, signer: &UrlSigner, current_unix_time: u64) -> bool {
        let full_uri = self.full_uri();
        signer.verify_at(&full_uri, current_unix_time).is_ok()
    }

    /// Assembles the relative path and query string into a complete URL string.
    #[must_use]
    pub fn full_uri(&self) -> String {
        match &self.query {
            Some(query) if !query.is_empty() => format!("{}?{}", self.path, query),
            _ => self.path.clone(),
        }
    }

    /// Applies HTTP method spoofing if `allow_spoofing` is enabled.
    pub fn resolve_spoofed_method(&mut self, allow_spoofing: bool) {
        if !allow_spoofing || self.intended_method != Method::Post {
            return;
        }

        // 1. Check header override
        if let Some(header_override) = self.headers.get(HeaderName::X_HTTP_METHOD_OVERRIDE) {
            let trimmed = header_override.trim();
            if !trimmed.is_empty() {
                self.method = Method::from(trimmed);
                return;
            }
        }

        // 2. Check query string override `_method`
        if let Some(method) = self
            .query
            .as_deref()
            .and_then(Self::extract_method_override)
        {
            self.method = method;
            return;
        }

        // 3. Check form body override `_method` if body is small urlencoded form
        if let Some(method) = std::str::from_utf8(&self.body)
            .ok()
            .and_then(Self::extract_method_override)
        {
            self.method = method;
        }
    }

    fn extract_method_override(payload: &str) -> Option<Method> {
        for pair in payload.split('&') {
            if let Some(raw_val) = pair.strip_prefix("_method=") {
                let decoded = percent_encoding::percent_decode_str(raw_val).decode_utf8_lossy();
                let trimmed = decoded.trim();
                if !trimmed.is_empty() {
                    return Some(Method::from(trimmed));
                }
            }
        }
        None
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Request")
            .field("method", &self.method)
            .field("intended_method", &self.intended_method)
            .field("path", &self.path)
            .field("query", &self.query)
            .field("headers", &self.headers)
            .field("params", &self.params)
            .field("body_len", &self.body.len())
            .field("client_ip", &self.client_ip)
            .finish()
    }
}

/// Fluent builder for constructing [`Request`] instances.
#[derive(Clone, Debug, Default)]
pub struct RequestBuilder {
    method: Option<Method>,
    path: Option<String>,
    query: Option<String>,
    headers: HeaderMap,
    params: RouteParams,
    body: Bytes,
    client_ip: Option<IpAddr>,
    trust_proxy: bool,
    keyring: Option<KeyRing>,
}

impl RequestBuilder {
    /// Creates a new `RequestBuilder`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the HTTP method for the request.
    #[must_use]
    pub fn method(mut self, method: Method) -> Self {
        self.method = Some(method);
        self
    }

    /// Sets the URI path for the request.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        let raw_path = path.into();
        if let Some((path_part, query_part)) = raw_path.split_once('?') {
            self.path = Some(path_part.to_string());
            self.query = Some(query_part.to_string());
        } else {
            self.path = Some(raw_path);
        }
        self
    }

    /// Sets the query string for the request.
    #[must_use]
    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Inserts a header onto the request.
    #[must_use]
    pub fn header(
        mut self,
        name: impl Into<HeaderName>,
        value: impl Into<crate::header::HeaderValue>,
    ) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Inserts an extracted path parameter.
    #[must_use]
    pub fn param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key, value);
        self
    }

    /// Sets the request payload body bytes.
    #[must_use]
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }

    /// Serializes a value as JSON into the request body.
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Result<Self, RouterError> {
        let bytes = serde_json::to_vec(value)?;
        self.headers
            .insert(HeaderName::CONTENT_TYPE, "application/json");
        self.body = Bytes::from(bytes);
        Ok(self)
    }

    /// Sets the client remote IP address.
    #[must_use]
    pub fn client_ip(mut self, ip: IpAddr) -> Self {
        self.client_ip = Some(ip);
        self
    }

    /// Enables or disables proxy trust header evaluation.
    #[must_use]
    pub fn trust_proxy(mut self, trust: bool) -> Self {
        self.trust_proxy = trust;
        self
    }

    /// Attaches a cryptographic key ring for encrypted/signed cookies.
    #[must_use]
    pub fn keyring(mut self, keyring: KeyRing) -> Self {
        self.keyring = Some(keyring);
        self
    }

    /// Builds and returns the finalized [`Request`].
    #[must_use]
    pub fn build(self) -> Request {
        let method = self.method.unwrap_or(Method::Get);
        let path = self.path.unwrap_or_else(|| "/".to_string());

        let cookie_header = self.headers.get(HeaderName::COOKIE);
        let cookie_jar = match self.keyring {
            Some(ring) => CookieJar::from_headers_with_keyring(cookie_header, ring),
            None => CookieJar::from_headers(cookie_header),
        };

        Request {
            method: method.clone(),
            intended_method: method,
            path,
            query: self.query,
            headers: self.headers,
            params: self.params,
            body: self.body,
            cookie_jar,
            client_ip: self.client_ip,
            trust_proxy: self.trust_proxy,
        }
    }
}
