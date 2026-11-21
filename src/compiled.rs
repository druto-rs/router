//! Immutable, thread-safe compiled routing dispatcher with zero-allocation execution.

use std::sync::Arc;

use dhap::{BoxFuture, BoxedMiddleware, Pipeline, Terminal};
use poth::{Error as PothError, Method, MethodMatcher};

use crate::{
    context::HttpContext,
    error::RouterError,
    handler::{BoxedHandler, Handler},
    header::HeaderName,
    into_response::IntoResponse,
    response::ResponseBody,
    route::Route,
    router::{ErrorHandlerFn, RouterConfig},
    status::StatusCode,
    url_generator::UrlGenerator,
};

/// Payload stored in the compiled route matcher containing the pre-assembled middleware pipeline.
#[derive(Clone)]
pub struct CompiledRoutePayload {
    pipeline: Arc<Pipeline<HttpContext, crate::response::Response, RouterError>>,
    terminal_handler: BoxedHandler,
    route_name: Option<String>,
}

struct HandlerTerminal {
    handler: BoxedHandler,
}

impl Terminal<HttpContext, crate::response::Response, RouterError> for HandlerTerminal {
    fn call(
        self,
        ctx: &mut HttpContext,
    ) -> BoxFuture<'_, Result<crate::response::Response, RouterError>> {
        Box::pin(async move {
            self.handler.call(ctx).await?;
            Ok(ctx.response.clone())
        })
    }
}

type FallbackRoute = (
    Arc<Pipeline<HttpContext, crate::response::Response, RouterError>>,
    BoxedHandler,
);

struct CompiledInner {
    matcher: MethodMatcher<CompiledRoutePayload>,
    fallback: Option<FallbackRoute>,
    error_handler: Option<ErrorHandlerFn>,
    url_generator: UrlGenerator,
    config: RouterConfig,
}

/// An immutable, thread-safe compiled routing engine ready for high-throughput dispatch.
#[derive(Clone)]
pub struct CompiledRouter {
    inner: Arc<CompiledInner>,
}

impl CompiledRouter {
    /// Compiles routes and middleware into a finalized [`CompiledRouter`].
    pub fn compile(
        routes: Vec<Route>,
        global_middlewares: Vec<
            BoxedMiddleware<HttpContext, crate::response::Response, RouterError>,
        >,
        fallback_handler: Option<BoxedHandler>,
        error_handler: Option<ErrorHandlerFn>,
        config: RouterConfig,
    ) -> Result<Self, RouterError> {
        let mut matcher: MethodMatcher<CompiledRoutePayload> = MethodMatcher::new();
        let mut url_matcher: MethodMatcher<()> = MethodMatcher::new();

        let global_stack = Arc::new(global_middlewares);

        for route in routes {
            let mut pipeline = Pipeline::new();

            for global_middleware in global_stack.iter() {
                pipeline.push_arc(Arc::clone(global_middleware));
            }

            for route_middleware in route.middlewares() {
                pipeline.push_arc(Arc::clone(route_middleware));
            }

            let payload = CompiledRoutePayload {
                pipeline: Arc::new(pipeline),
                terminal_handler: Arc::clone(route.handler()),
                route_name: route.route_name().map(ToString::to_string),
            };

            let constraints = route.constraints().clone();

            for method in route.methods() {
                if let Some(name) = route.route_name() {
                    matcher.insert_named_with_constraints(
                        name,
                        method.clone(),
                        route.pattern(),
                        payload.clone(),
                        constraints.clone(),
                    )?;
                    url_matcher.insert_named_with_constraints(
                        name,
                        method.clone(),
                        route.pattern(),
                        (),
                        constraints.clone(),
                    )?;
                } else if *method == Method::Any {
                    matcher.insert_with_constraints(
                        Method::Any,
                        route.pattern(),
                        payload.clone(),
                        constraints.clone(),
                    )?;
                    url_matcher.insert_with_constraints(
                        Method::Any,
                        route.pattern(),
                        (),
                        constraints.clone(),
                    )?;
                } else {
                    matcher.insert_with_constraints(
                        method.clone(),
                        route.pattern(),
                        payload.clone(),
                        constraints.clone(),
                    )?;
                    url_matcher.insert_with_constraints(
                        method.clone(),
                        route.pattern(),
                        (),
                        constraints.clone(),
                    )?;
                }
            }
        }

        let mut url_generator = UrlGenerator::new(Arc::new(url_matcher));
        if let Some(signer) = config.url_signer.clone() {
            url_generator = url_generator.with_signer(signer);
        }
        if let Some(ref base_url) = config.base_url {
            url_generator = url_generator.with_base_url(base_url.clone());
        }

        let fallback = fallback_handler.map(|handler| {
            let mut pipeline = Pipeline::new();
            for global_middleware in global_stack.iter() {
                pipeline.push_arc(Arc::clone(global_middleware));
            }
            (Arc::new(pipeline), handler)
        });

        Ok(Self {
            inner: Arc::new(CompiledInner {
                matcher,
                fallback,
                error_handler,
                url_generator,
                config,
            }),
        })
    }

    /// Returns a reference to the reverse route and signed URL generator.
    #[must_use]
    pub fn url_generator(&self) -> &UrlGenerator {
        &self.inner.url_generator
    }

    /// Dispatches an incoming request context through the pre-compiled route pipeline.
    pub async fn dispatch(&self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        let dispatch_result = self.execute_dispatch(ctx).await;

        if let Err(error) = dispatch_result {
            if let Some(ref custom_error_handler) = self.inner.error_handler {
                custom_error_handler(ctx, error).await?;
            } else {
                error.apply(ctx)?;
            }
        }

        // RFC 9110 HEAD method: suppress response body
        if *ctx.request.method() == Method::Head {
            ctx.response.set_body(ResponseBody::Empty);
        }

        ctx.flush_cookies_to_response();
        Ok(())
    }

    async fn execute_dispatch(&self, ctx: &mut HttpContext) -> Result<(), RouterError> {
        // 1. Guard request payload byte ceiling
        let body_len = ctx.request.body().len();
        if body_len > self.inner.config.max_body_size {
            return Err(RouterError::PayloadTooLarge {
                max_bytes: self.inner.config.max_body_size,
                received_bytes: body_len,
            });
        }

        // 2. Resolve method spoofing if enabled
        ctx.request
            .resolve_spoofed_method(self.inner.config.allow_method_spoofing);

        let method = ctx.request.method().clone();
        let path = ctx.request.path().to_string();

        // 3. Match against compiled route trie
        match self.inner.matcher.find(&method, &path) {
            Ok(matched) => {
                // Populate path parameters
                for (key, value) in matched.params.iter() {
                    ctx.request.params_mut().insert(key, value);
                }

                let payload = matched.payload.clone();
                ctx.set_matched_route_name(payload.route_name.clone());

                let terminal = HandlerTerminal {
                    handler: Arc::clone(&payload.terminal_handler),
                };

                payload.pipeline.run(ctx, terminal).await?;
                Ok(())
            }
            Err(PothError::MethodNotAllowed { allowed, .. }) => {
                // RFC 9110 automatic OPTIONS response when no explicit OPTIONS route exists
                if method == Method::Options {
                    let mut all_allowed = allowed;
                    if !all_allowed.contains(&Method::Options) {
                        all_allowed.push(Method::Options);
                    }
                    if all_allowed.contains(&Method::Get) && !all_allowed.contains(&Method::Head) {
                        all_allowed.push(Method::Head);
                    }
                    let allowed_str = all_allowed
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");

                    ctx.response.set_status(StatusCode::NO_CONTENT);
                    ctx.response.set_header(HeaderName::ALLOW, allowed_str);
                    ctx.response.set_body(ResponseBody::Empty);
                    Ok(())
                } else {
                    Err(RouterError::MethodNotAllowed { allowed })
                }
            }
            Err(PothError::NotFound { .. }) => {
                if let Some((ref fallback_pipeline, ref fallback_handler)) = self.inner.fallback {
                    let terminal = HandlerTerminal {
                        handler: Arc::clone(fallback_handler),
                    };
                    fallback_pipeline.run(ctx, terminal).await?;
                    Ok(())
                } else {
                    Err(RouterError::NotFound)
                }
            }
            Err(other) => Err(other.into()),
        }
    }
}
