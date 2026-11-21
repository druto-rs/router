//! Declarative route manifest macros.

/// Declarative macro for constructing a [`Router`](crate::Router) with a manifest DSL.
///
/// # Examples
///
/// ```rust
/// use druto_router::{BoxFuture, HttpContext, RouterError, routes};
///
/// fn hello(ctx: &mut HttpContext) -> BoxFuture<'_, Result<(), RouterError>> {
///     let _ = ctx;
///     Box::pin(async move {
///         Ok(())
///     })
/// }
///
/// let router = routes! {
///     GET "/hello" => hello,
/// };
/// ```
#[macro_export]
macro_rules! routes {
    ( $( $tokens:tt )* ) => {{
        let mut druto_router = $crate::Router::new();
        $crate::routes_step!(druto_router; $( $tokens )*);
        druto_router
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! routes_step {
    // Base case: empty
    ($router:ident;) => {};

    // Trailing comma
    ($router:ident; , $( $rest:tt )*) => {
        $crate::routes_step!($router; $( $rest )*);
    };

    // GET with name
    ($router:ident; GET $path:expr => $handler:expr, as: $name:expr, $( $rest:tt )*) => {
        $router.get($path, $handler).r#as($name);
        $crate::routes_step!($router; $( $rest )*);
    };

    // GET
    ($router:ident; GET $path:expr => $handler:expr, $( $rest:tt )*) => {
        $router.get($path, $handler);
        $crate::routes_step!($router; $( $rest )*);
    };

    // GET (no trailing comma)
    ($router:ident; GET $path:expr => $handler:expr) => {
        $router.get($path, $handler);
    };

    // POST with name
    ($router:ident; POST $path:expr => $handler:expr, as: $name:expr, $( $rest:tt )*) => {
        $router.post($path, $handler).r#as($name);
        $crate::routes_step!($router; $( $rest )*);
    };

    // POST
    ($router:ident; POST $path:expr => $handler:expr, $( $rest:tt )*) => {
        $router.post($path, $handler);
        $crate::routes_step!($router; $( $rest )*);
    };

    // POST (no trailing comma)
    ($router:ident; POST $path:expr => $handler:expr) => {
        $router.post($path, $handler);
    };

    // PUT
    ($router:ident; PUT $path:expr => $handler:expr, $( $rest:tt )*) => {
        $router.put($path, $handler);
        $crate::routes_step!($router; $( $rest )*);
    };

    // PUT (no trailing comma)
    ($router:ident; PUT $path:expr => $handler:expr) => {
        $router.put($path, $handler);
    };

    // PATCH
    ($router:ident; PATCH $path:expr => $handler:expr, $( $rest:tt )*) => {
        $router.patch($path, $handler);
        $crate::routes_step!($router; $( $rest )*);
    };

    // PATCH (no trailing comma)
    ($router:ident; PATCH $path:expr => $handler:expr) => {
        $router.patch($path, $handler);
    };

    // DELETE
    ($router:ident; DELETE $path:expr => $handler:expr, $( $rest:tt )*) => {
        $router.delete($path, $handler);
        $crate::routes_step!($router; $( $rest )*);
    };

    // DELETE (no trailing comma)
    ($router:ident; DELETE $path:expr => $handler:expr) => {
        $router.delete($path, $handler);
    };

    // ANY
    ($router:ident; ANY $path:expr => $handler:expr, $( $rest:tt )*) => {
        $router.any($path, $handler);
        $crate::routes_step!($router; $( $rest )*);
    };

    // ANY (no trailing comma)
    ($router:ident; ANY $path:expr => $handler:expr) => {
        $router.any($path, $handler);
    };

    // REDIRECT with explicit status
    ($router:ident; REDIRECT $from:expr => $to:expr, status: $status:expr, $( $rest:tt )*) => {
        $router.redirect($from, $to, $status);
        $crate::routes_step!($router; $( $rest )*);
    };

    // REDIRECT with explicit status (no trailing comma)
    ($router:ident; REDIRECT $from:expr => $to:expr, status: $status:expr) => {
        $router.redirect($from, $to, $status);
    };

    // REDIRECT (defaults to 302 Found)
    ($router:ident; REDIRECT $from:expr => $to:expr, $( $rest:tt )*) => {
        $router.redirect($from, $to, $crate::StatusCode::FOUND);
        $crate::routes_step!($router; $( $rest )*);
    };

    // REDIRECT (defaults to 302 Found, no trailing comma)
    ($router:ident; REDIRECT $from:expr => $to:expr) => {
        $router.redirect($from, $to, $crate::StatusCode::FOUND);
    };

    // RESOURCE api_only
    ($router:ident; RESOURCE $name:expr => $controller:expr, api_only, $( $rest:tt )*) => {
        $router.resource_with($name, $controller, |res| {
            res.api_only();
        });
        $crate::routes_step!($router; $( $rest )*);
    };

    // RESOURCE standard
    ($router:ident; RESOURCE $name:expr => $controller:expr, $( $rest:tt )*) => {
        $router.resource($name, $controller);
        $crate::routes_step!($router; $( $rest )*);
    };

    // NEST with name
    ($router:ident; NEST $prefix:expr => $sub:expr, as: $name:expr, $( $rest:tt )*) => {
        $router.nest_as($prefix, $name, $sub);
        $crate::routes_step!($router; $( $rest )*);
    };

    // NEST
    ($router:ident; NEST $prefix:expr => $sub:expr, $( $rest:tt )*) => {
        $router.nest($prefix, $sub);
        $crate::routes_step!($router; $( $rest )*);
    };

    // FALLBACK
    ($router:ident; FALLBACK => $handler:expr, $( $rest:tt )*) => {
        $router.fallback($handler);
        $crate::routes_step!($router; $( $rest )*);
    };

    // FALLBACK (no trailing comma)
    ($router:ident; FALLBACK => $handler:expr) => {
        $router.fallback($handler);
    };

    // GROUP with =>
    ($router:ident; GROUP $prefix:expr => { $( $inner:tt )* }, $( $rest:tt )*) => {
        $router.group_prefixed($prefix, |group| {
            $crate::routes_step!(group; $( $inner )*);
        });
        $crate::routes_step!($router; $( $rest )*);
    };

    // GROUP with => (no trailing comma)
    ($router:ident; GROUP $prefix:expr => { $( $inner:tt )* }) => {
        $router.group_prefixed($prefix, |group| {
            $crate::routes_step!(group; $( $inner )*);
        });
    };
}
