//! Reverse URL generation, named route resolution, and cryptographic URL signing.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use gopon::UrlSigner;
use poth::MethodMatcher;

use crate::error::RouterError;

/// Generator for resolving named routes and generating cryptographically signed URLs.
#[derive(Clone, Debug)]
pub struct UrlGenerator {
    matcher: Arc<MethodMatcher<()>>,
    signer: Option<UrlSigner>,
    base_url: Option<String>,
}

impl UrlGenerator {
    /// Creates a new `UrlGenerator` bound to the compiled route pattern registry.
    #[must_use]
    pub fn new(matcher: Arc<MethodMatcher<()>>) -> Self {
        Self {
            matcher,
            signer: None,
            base_url: None,
        }
    }

    /// Attaches an HMAC URL signer for generating signed URLs.
    #[must_use]
    pub fn with_signer(mut self, signer: UrlSigner) -> Self {
        self.signer = Some(signer);
        self
    }

    /// Attaches an optional base URL prefix (e.g. `https://example.com`) for absolute URLs.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let trimmed = base_url.into().trim_end_matches('/').to_string();
        self.base_url = Some(trimmed);
        self
    }

    /// Resolves a named route pattern and builds its relative path substituting dynamic parameters.
    pub fn url_for(&self, name: &str, params: &[(&str, &str)]) -> Result<String, RouterError> {
        let relative_path = self.matcher.build_path(name, params)?;
        if let Some(ref base) = self.base_url {
            Ok(format!("{base}{relative_path}"))
        } else {
            Ok(relative_path)
        }
    }

    /// Generates a signed URL with a persistent HMAC signature query parameter.
    pub fn signed_url_for(
        &self,
        name: &str,
        params: &[(&str, &str)],
    ) -> Result<String, RouterError> {
        let signer = self.signer.as_ref().ok_or_else(|| {
            RouterError::Internal(
                "Cannot generate signed URL: No UrlSigner configured on UrlGenerator".to_string(),
            )
        })?;
        let url = self.url_for(name, params)?;
        Ok(signer.sign(&url))
    }

    /// Generates a temporary signed URL valid for `ttl_seconds` from the current time.
    pub fn temporary_signed_url_for(
        &self,
        name: &str,
        params: &[(&str, &str)],
        ttl_seconds: u64,
    ) -> Result<String, RouterError> {
        let signer = self.signer.as_ref().ok_or_else(|| {
            RouterError::Internal(
                "Cannot generate temporary signed URL: No UrlSigner configured on UrlGenerator"
                    .to_string(),
            )
        })?;
        let url = self.url_for(name, params)?;

        let current_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires_at = current_unix.saturating_add(ttl_seconds);

        Ok(signer.sign_temporary(&url, expires_at))
    }

    /// Returns a reference to the attached cryptographic URL signer, if configured.
    #[must_use]
    pub fn signer(&self) -> Option<&UrlSigner> {
        self.signer.as_ref()
    }
}
