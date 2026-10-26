//! Logging, metrics and the HTTP metrics middleware (see docs/architecture/observability.md).
//!
//! Metric labels never carry raw paths, user input or IDs. Routes use the route template (`/v1/messages/{id}`).

use std::{net::SocketAddr, time::Instant};

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use tracing_subscriber::EnvFilter;

use crate::config::{Config, LogFormat};

/// Route label used when no route matched. Keeps raw paths out of metric labels.
pub const UNMATCHED_ROUTE: &str = "unmatched";

/// Installs the global tracing subscriber. Safe to call more than once; later calls are ignored.
pub fn init_tracing(config: &Config) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    let _ = match config.log_format {
        LogFormat::Json => builder.json().try_init(),
        LogFormat::Pretty => builder.try_init(),
    };
}

/// Installs the global metrics recorder and serves `/metrics` on `addr`. Call once, from `serve`.
pub fn init_metrics(addr: SocketAddr) -> anyhow::Result<()> {
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()?;
    metrics::gauge!(
        "build_info",
        "version" => env!("CARGO_PKG_VERSION"),
        "git_sha" => option_env!("GIT_SHA").unwrap_or("unknown"),
    )
    .set(1.0);
    Ok(())
}

/// Returns the label for a matched route template, or `UNMATCHED_ROUTE`.
pub fn route_label(matched: Option<&MatchedPath>) -> &str {
    matched.map(MatchedPath::as_str).unwrap_or(UNMATCHED_ROUTE)
}

/// Records `http_requests_total` and `http_request_duration_seconds` for each request.
///
/// Apply with `route_layer`, so the route template is available. Requests that match no route are not recorded.
pub async fn http_metrics(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().as_str().to_owned();
    let route = route_label(request.extensions().get::<MatchedPath>()).to_owned();

    let response = next.run(request).await;

    let status = response.status().as_u16().to_string();
    metrics::counter!(
        "http_requests_total",
        "route" => route.clone(),
        "method" => method,
        "status" => status,
    )
    .increment(1);
    metrics::histogram!("http_request_duration_seconds", "route" => route)
        .record(started.elapsed().as_secs_f64());
    response
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn unmatched_requests_get_a_fixed_label() {
        assert_eq!(route_label(None), "unmatched");
    }

    #[tokio::test]
    async fn matched_route_exposes_its_template_not_the_raw_path() {
        // The label comes from MatchedPath. Check that it is the template, so raw IDs never reach a label.
        async fn echo(matched: MatchedPath) -> String {
            matched.as_str().to_owned()
        }
        let app = Router::new()
            .route("/v1/messages/{id}", get(echo))
            .route_layer(from_fn(http_metrics));

        let response = app
            .oneshot(
                Request::get("/v1/messages/123e4567-e89b-12d3-a456-426614174000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"/v1/messages/{id}");
    }
}
