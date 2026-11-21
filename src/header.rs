//! HTTP headers modeling, case-insensitive lookup, and multi-value storage.

use std::{borrow::Cow, collections::HashMap, fmt, str::FromStr};

/// A case-insensitive HTTP header name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HeaderName(Cow<'static, str>);

impl HeaderName {
    /// `Content-Type`
    pub const CONTENT_TYPE: Self = Self(Cow::Borrowed("content-type"));
    /// `Content-Length`
    pub const CONTENT_LENGTH: Self = Self(Cow::Borrowed("content-length"));
    /// `Allow`
    pub const ALLOW: Self = Self(Cow::Borrowed("allow"));
    /// `Location`
    pub const LOCATION: Self = Self(Cow::Borrowed("location"));
    /// `Set-Cookie`
    pub const SET_COOKIE: Self = Self(Cow::Borrowed("set-cookie"));
    /// `Cookie`
    pub const COOKIE: Self = Self(Cow::Borrowed("cookie"));
    /// `Authorization`
    pub const AUTHORIZATION: Self = Self(Cow::Borrowed("authorization"));
    /// `Accept`
    pub const ACCEPT: Self = Self(Cow::Borrowed("accept"));
    /// `Host`
    pub const HOST: Self = Self(Cow::Borrowed("host"));
    /// `Forwarded`
    pub const FORWARDED: Self = Self(Cow::Borrowed("forwarded"));
    /// `X-Forwarded-For`
    pub const X_FORWARDED_FOR: Self = Self(Cow::Borrowed("x-forwarded-for"));
    /// `X-Forwarded-Host`
    pub const X_FORWARDED_HOST: Self = Self(Cow::Borrowed("x-forwarded-host"));
    /// `X-Forwarded-Proto`
    pub const X_FORWARDED_PROTO: Self = Self(Cow::Borrowed("x-forwarded-proto"));
    /// `X-Real-IP`
    pub const X_REAL_IP: Self = Self(Cow::Borrowed("x-real-ip"));
    /// `X-HTTP-Method-Override`
    pub const X_HTTP_METHOD_OVERRIDE: Self = Self(Cow::Borrowed("x-http-method-override"));
    /// `Vary`
    pub const VARY: Self = Self(Cow::Borrowed("vary"));

    /// Creates a new `HeaderName` in lowercase for case-insensitive matching.
    #[must_use]
    pub fn new(name: impl AsRef<str>) -> Self {
        Self(Cow::Owned(name.as_ref().to_ascii_lowercase()))
    }

    /// Creates a static `HeaderName` without heap allocation.
    #[must_use]
    pub const fn from_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// Returns the lowercase string slice of the header name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for HeaderName {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for HeaderName {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

impl FromStr for HeaderName {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}

/// An HTTP header value preserving original character casing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HeaderValue(String);

impl HeaderValue {
    /// Creates a new `HeaderValue`.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the string slice of the header value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HeaderValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for HeaderValue {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for HeaderValue {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A map of HTTP headers supporting case-insensitive keys and multiple values per key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeaderMap {
    entries: HashMap<HeaderName, Vec<HeaderValue>>,
}

impl HeaderMap {
    /// Creates an empty `HeaderMap`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Inserts a header, replacing any existing values for that header name.
    pub fn insert(&mut self, name: impl Into<HeaderName>, value: impl Into<HeaderValue>) {
        self.entries.insert(name.into(), vec![value.into()]);
    }

    /// Appends a header value to the list of values associated with the header name.
    pub fn append(&mut self, name: impl Into<HeaderName>, value: impl Into<HeaderValue>) {
        self.entries
            .entry(name.into())
            .or_default()
            .push(value.into());
    }

    /// Returns the first value associated with the header name, if present.
    #[must_use]
    pub fn get(&self, name: impl Into<HeaderName>) -> Option<&str> {
        let key = name.into();
        self.entries
            .get(&key)
            .and_then(|values| values.first())
            .map(HeaderValue::as_str)
    }

    /// Returns an iterator over all values associated with the header name.
    pub fn get_all(&self, name: impl Into<HeaderName>) -> impl Iterator<Item = &str> {
        let key = name.into();
        let values = self.entries.get(&key).map_or(&[][..], Vec::as_slice);
        values.iter().map(HeaderValue::as_str)
    }

    /// Removes all values associated with the header name.
    pub fn remove(&mut self, name: impl Into<HeaderName>) -> Option<Vec<HeaderValue>> {
        let key = name.into();
        self.entries.remove(&key)
    }

    /// Checks if a header name is present in the map.
    #[must_use]
    pub fn contains_key(&self, name: impl Into<HeaderName>) -> bool {
        let key = name.into();
        self.entries.contains_key(&key)
    }

    /// Returns the number of distinct header names stored in the map.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the map contains no headers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears all headers from the map.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns an iterator over all distinct header entries.
    pub fn iter(&self) -> impl Iterator<Item = (&HeaderName, &[HeaderValue])> {
        self.entries
            .iter()
            .map(|(key, values)| (key, values.as_slice()))
    }
}

impl<K, V> FromIterator<(K, V)> for HeaderMap
where
    K: Into<HeaderName>,
    V: Into<HeaderValue>,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut map = Self::new();
        for (name, value) in iter {
            map.append(name, value);
        }
        map
    }
}
