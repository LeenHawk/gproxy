//! The static console, with an SPA fallback, behind the configuration switch.
//!
//! Two sources, decided once at startup:
//!
//! - the bundle embedded in the binary (`assets/web`, filled by the release
//!   build from `console/dist`);
//! - a directory named by
//!   [`ConsoleConfig::path`](gproxy_app::config::ConsoleConfig), which is what
//!   a developer points at a running Vite build.
//!
//! **A source checkout embeds nothing**, and that is the intended state: a
//! `cargo test` binary has no console in it, so every console path answers
//! 404 rather than a blank page that looks like a broken application.
//!
//! # The SPA fallback, and its one limit
//!
//! A single-page application owns its own routes, so `/console/keys` has to
//! return `index.html` for the router to pick it up. That is only done for a
//! request that **looks like a document**: a `GET` whose path has no file
//! extension. A missing `/assets/app-a1b2c3.js` must answer 404, not an HTML
//! page — a browser handed HTML where it expected a module logs a MIME error
//! that says nothing about the real problem, which is that the bundle is
//! stale.

use axum::response::{IntoResponse, Response};
use gproxy_app::config::ConsoleConfig;
use http::{HeaderValue, Method, StatusCode, header};
use rust_embed::RustEmbed;

/// Where the console is served from, `/console` and below.
pub const CONSOLE_PATH: &str = "/console";

#[derive(RustEmbed)]
#[folder = "assets/web"]
#[exclude = ".gitkeep"]
struct Embedded;

/// The console bundle this instance serves, resolved once.
#[derive(Debug)]
pub enum Console {
    /// The switch is off, or nothing was embedded and no directory was named.
    Disabled,
    /// The bundle compiled into this binary.
    Embedded,
    /// A directory on disk, for development.
    Directory(std::path::PathBuf),
}

impl Console {
    pub fn from_config(config: &ConsoleConfig) -> Self {
        if !config.enabled {
            return Self::Disabled;
        }
        match config.path.as_deref() {
            Some(path) if !path.trim().is_empty() => Self::Directory(path.trim().into()),
            // `index.html` is the one file whose presence proves a bundle was
            // built into this binary; `Embedded::iter()` would be true for a
            // stray asset.
            _ if Embedded::get("index.html").is_some() => Self::Embedded,
            _ => Self::Disabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Serve `path` (the request path, mount-relative), or `None` when this is
    /// not a console request at all — which lets the caller carry on to
    /// whatever comes after the console in the ingress order.
    pub async fn serve(&self, method: &Method, path: &str) -> Option<Response> {
        let asset = asset_name(method, path)?;
        if !self.is_enabled() {
            return Some(not_found("the console is not enabled on this instance"));
        }
        let head = method == Method::HEAD;
        match self.read(&asset).await {
            Some(bytes) => Some(asset_response(&asset, bytes, head)),
            // The SPA fallback: a document request for a route the bundle
            // owns. An asset request that missed stays a 404.
            None if is_document(&asset) => match self.read("index.html").await {
                Some(bytes) => Some(asset_response("index.html", bytes, head)),
                None => Some(not_found("the console bundle is missing index.html")),
            },
            None => Some(not_found("not found")),
        }
    }

    async fn read(&self, asset: &str) -> Option<Vec<u8>> {
        match self {
            Self::Disabled => None,
            Self::Embedded => Embedded::get(asset).map(|file| file.data.into_owned()),
            Self::Directory(root) => {
                // `asset_name` has already rejected `..` and absolute paths, so
                // the join cannot leave the root.
                tokio::fs::read(root.join(asset)).await.ok()
            }
        }
    }
}

/// The file `path` asks for, or `None` when the request is not the console's.
///
/// Rejects anything with a `..` segment or a leading `/` after the prefix is
/// stripped: the directory source joins this onto a root, and a traversal
/// there would serve the instance's own files.
fn asset_name(method: &Method, path: &str) -> Option<String> {
    if method != Method::GET && method != Method::HEAD {
        return None;
    }
    let rest = match path {
        "/" => "",
        // Exactly one separator is stripped, not every leading one: `//etc/`
        // would otherwise lose both and become the absolute path `etc/` with
        // no empty segment left for the check below to catch.
        path => match path.strip_prefix(CONSOLE_PATH)? {
            "" => "",
            rest => rest.strip_prefix('/')?,
        },
    };
    if rest.is_empty() {
        return Some("index.html".to_owned());
    }
    (!rest.split('/').any(|segment| {
        segment.is_empty() || segment == ".." || segment == "." || segment.contains('\\')
    }))
    .then(|| rest.to_owned())
}

/// Whether a miss should fall back to `index.html`. A path whose last segment
/// carries an extension is asking for a file, not for a route.
fn is_document(asset: &str) -> bool {
    !asset
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
}

fn asset_response(asset: &str, bytes: Vec<u8>, head: bool) -> Response {
    let body = if head { Vec::new() } else { bytes };
    let mut response = axum::body::Body::from(body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(
            mime_guess::from_path(asset)
                .first_raw()
                .unwrap_or("application/octet-stream"),
        )
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        if asset == "index.html" {
            // The document names the hashed bundles, so it must never be the
            // stale half of a deployment.
            HeaderValue::from_static("no-cache")
        } else {
            HeaderValue::from_static("public, max-age=31536000, immutable")
        },
    );
    response
}

fn not_found(message: &'static str) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_document_requests_under_the_prefix_are_the_consoles() {
        assert_eq!(asset_name(&Method::GET, "/"), Some("index.html".into()));
        assert_eq!(
            asset_name(&Method::GET, "/console"),
            Some("index.html".into())
        );
        assert_eq!(
            asset_name(&Method::GET, "/console/"),
            Some("index.html".into())
        );
        assert_eq!(
            asset_name(&Method::GET, "/console/assets/app.js"),
            Some("assets/app.js".into())
        );
        // Not the console's: another surface owns these.
        assert_eq!(asset_name(&Method::GET, "/v1/messages"), None);
        assert_eq!(asset_name(&Method::GET, "/consolexyz"), None);
        // Not a readable method.
        assert_eq!(asset_name(&Method::POST, "/console"), None);
    }

    #[test]
    fn a_traversal_is_not_an_asset_name() {
        for path in [
            "/console/../Cargo.toml",
            "/console/assets/../../etc/passwd",
            "/console/a/./b",
            "/console//etc/passwd",
        ] {
            assert_eq!(asset_name(&Method::GET, path), None, "{path}");
        }
    }

    #[test]
    fn a_route_falls_back_to_the_document_and_a_missing_asset_does_not() {
        assert!(is_document("keys"));
        assert!(is_document("organizations/o1/members"));
        assert!(!is_document("assets/app-a1b2c3.js"));
        assert!(!is_document("favicon.ico"));
        assert!(!is_document("index.html"));
    }

    #[tokio::test]
    async fn a_checkout_with_no_bundle_answers_404_rather_than_a_blank_page() {
        // Whether a bundle exists is not a property of the source tree: in a
        // debug build `rust_embed` reads `assets/web` from disk, so this is
        // `Embedded` for anyone who has run the console's build and `Disabled`
        // for everyone else. Asserting the machine's current state would make
        // `cargo test` fail for whoever built the console last. The rule is
        // what holds in both worlds: asking for the console gets it exactly
        // when there is one to get.
        let console = Console::from_config(&ConsoleConfig {
            enabled: true,
            path: None,
        });
        assert_eq!(
            console.is_enabled(),
            Embedded::get("index.html").is_some(),
            "the console is enabled if and only if a bundle is embedded"
        );

        // The answer when there is nothing to serve, which is the part that
        // must never become a blank page.
        let empty = Console::Disabled;
        let response = empty.serve(&Method::GET, "/console").await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        // And a non-console path is still nobody's.
        assert!(empty.serve(&Method::GET, "/v1/messages").await.is_none());
    }

    #[tokio::test]
    async fn the_switch_turns_it_off_even_with_a_directory() {
        let console = Console::from_config(&ConsoleConfig {
            enabled: false,
            path: Some("/srv/console".into()),
        });
        assert!(matches!(console, Console::Disabled));
    }
}
