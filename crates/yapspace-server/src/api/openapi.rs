//! OpenAPI document for the API (see docs/architecture/api.md §F.4). Paths are added next to their handlers with
//! `routes!`. This module holds the document-level metadata and the security scheme.

use utoipa::{
    Modify, OpenApi,
    openapi::{
        OpenApi as OpenApiDoc,
        security::{Http, HttpAuthScheme, SecurityScheme},
    },
};

/// Name of the bearer security scheme used by protected endpoints.
pub const SESSION_TOKEN_SCHEME: &str = "session_token";

#[derive(OpenApi)]
#[openapi(
    info(
        title = "YapSpace API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Public API for YapSpace, the shared discussion layer for web pages."
    ),
    modifiers(&SessionToken)
)]
pub struct ApiDoc;

/// Adds the `session_token` bearer scheme to the components.
struct SessionToken;

impl Modify for SessionToken {
    fn modify(&self, openapi: &mut OpenApiDoc) {
        // utoipa adds an empty license object by default. This project has no public license, so drop it.
        openapi.info.license = None;
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            SESSION_TOKEN_SCHEME,
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );
    }
}

/// The complete document: metadata plus every path registered through `routes!`.
pub fn document() -> OpenApiDoc {
    crate::api::router().split_for_parts().1
}

/// The document as pretty-printed JSON, with a trailing newline. This is what `yapspace openapi --out` writes.
pub fn to_pretty_json() -> anyhow::Result<String> {
    Ok(format!("{}\n", document().to_pretty_json()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_has_title_version_bearer_scheme_and_health_paths() {
        let doc = document();
        assert!(doc.paths.paths.contains_key("/healthz"));
        assert!(doc.paths.paths.contains_key("/readyz"));
        assert_eq!(doc.info.title, "YapSpace API");
        assert_eq!(doc.info.version, env!("CARGO_PKG_VERSION"));
        let schemes = &doc.components.as_ref().expect("components").security_schemes;
        assert!(schemes.contains_key(SESSION_TOKEN_SCHEME));
    }
}
