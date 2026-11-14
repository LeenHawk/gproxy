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
//! An administrator can also upload a bundle of their own ([`custom`]), which
//! is served in place of either source until it is removed. The built-in
//! console stays reachable behind `?console=builtin` (see [`Console::serve`]).
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
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
#[cfg(feature = "embedded-console")]
use rust_embed::RustEmbed;

pub mod custom;
pub mod fonts;

/// Where the console is served from, `/console` and below.
pub const CONSOLE_PATH: &str = "/console";

#[cfg(feature = "embedded-console")]
#[derive(RustEmbed)]
#[folder = "assets/web"]
#[exclude = ".gitkeep"]
struct Embedded;

/// The cookie that pins a browser to the built-in console while an uploaded
/// one is installed. Set by `?console=builtin`, cleared by `?console=custom`.
pub const BUILTIN_COOKIE: &str = "gproxy_console";

/// The console bundle this instance serves, resolved once.
#[derive(Debug)]
pub struct Console {
    /// The configuration switch. Off means nothing below is served.
    enabled: bool,
    source: Source,
    custom: custom::CustomBundle,
    fonts: fonts::FontCache,
}

#[derive(Debug)]
enum Source {
    /// Nothing was embedded and no directory was named.
    Disabled,
    /// The bundle compiled into this binary.
    #[cfg(feature = "embedded-console")]
    Embedded,
    /// A directory on disk, for development.
    Directory(std::path::PathBuf),
}

impl Console {
    pub fn from_config(config: &ConsoleConfig) -> Self {
        let source = match (config.enabled, config.path.as_deref()) {
            (false, _) => Source::Disabled,
            (true, Some(path)) if !path.trim().is_empty() => Source::Directory(path.trim().into()),
            // `index.html` is the one file whose presence proves a bundle was
            // built into this binary; `Embedded::iter()` would be true for a
            // stray asset.
            #[cfg(feature = "embedded-console")]
            _ if Embedded::get("index.html").is_some() => Source::Embedded,
            _ => Source::Disabled,
        };
        Self {
            enabled: config.enabled,
            source,
            custom: custom::CustomBundle::new("data/console".into()),
            fonts: fonts::FontCache::new("data/fonts".into()),
        }
    }

    pub fn with_font_cache(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.fonts = fonts::FontCache::new(path.into());
        self
    }

    pub fn with_custom_bundle(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.custom = custom::CustomBundle::new(path.into());
        self
    }

    pub fn fonts(&self) -> &fonts::FontCache {
        &self.fonts
    }

    pub fn custom(&self) -> &custom::CustomBundle {
        &self.custom
    }

    pub async fn custom_status(&self) -> custom::ConsoleBundleStatus {
        self.custom.status(self.has_builtin()).await
    }

    pub async fn download_fonts(&self) -> std::io::Result<fonts::FontStatus> {
        let manifest = self
            .read_builtin("fonts/manifest.json")
            .await
            .ok_or_else(|| std::io::Error::other("Console font manifest is unavailable"))?;
        let package: fonts::FontPackage =
            serde_json::from_slice(&manifest).map_err(std::io::Error::other)?;
        let css = self
            .read_builtin(&format!("fonts/{}", package.stylesheet))
            .await
            .ok_or_else(|| std::io::Error::other("Console font stylesheet is unavailable"))?;
        self.fonts.download(&package, &css).await
    }

    /// Whether `/console` serves anything at all: the switch is on and there
    /// is a built-in or an uploaded bundle.
    pub fn is_enabled(&self) -> bool {
        self.enabled && (self.has_builtin() || self.custom.is_active())
    }

    /// Whether an administrator may upload a bundle: the configuration switch
    /// is on, whether or not this binary embeds a console of its own.
    pub fn accepts_uploads(&self) -> bool {
        self.enabled
    }

    fn has_builtin(&self) -> bool {
        !matches!(self.source, Source::Disabled)
    }

    /// Serve `path` (the request path, mount-relative), or `None` when this is
    /// not a console request at all — which lets the caller carry on to
    /// whatever comes after the console in the ingress order.
    ///
    /// With an uploaded bundle installed, it serves every request unless the
    /// browser asked for the built-in console: `?console=builtin` on any
    /// console URL sets [`BUILTIN_COOKIE`] and `?console=custom` clears it.
    /// That switch is what keeps the built-in console's settings page (and its
    /// remove button) reachable when the uploaded bundle is broken.
    pub async fn serve(
        &self,
        method: &Method,
        path: &str,
        query: Option<&str>,
        headers: &HeaderMap,
    ) -> Option<Response> {
        let asset = asset_name(method, path)?;
        if !self.is_enabled() {
            return Some(not_found("the console is not enabled on this instance"));
        }
        let head = method == Method::HEAD;
        if asset == "fonts/active.css" {
            let bytes = self.fonts.stylesheet().await.unwrap_or_default();
            let mut response = asset_response(&asset, bytes, head);
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            return Some(response);
        }
        if let Some(filename) = fonts::filename(&asset) {
            return Some(match self.fonts.read(filename).await {
                Some(bytes) => asset_response(&asset, bytes, head),
                None => not_found("font has not been downloaded"),
            });
        }
        let switch = requested_switch(query);
        let wants_builtin = switch.unwrap_or_else(|| has_builtin_cookie(headers));
        let custom = self.custom.is_active() && !(wants_builtin && self.has_builtin());
        let read = |asset: String| async move {
            if custom {
                self.custom.read(&asset).await
            } else {
                self.read_builtin(&asset).await
            }
        };
        let mut response = match read(asset.clone()).await {
            Some(bytes) => asset_response(&asset, bytes, head),
            // The SPA fallback: a document request for a route the bundle
            // owns. An asset request that missed stays a 404.
            None if is_document(&asset) => match read("index.html".to_owned()).await {
                Some(bytes) => asset_response("index.html", bytes, head),
                None => not_found("the console bundle is missing index.html"),
            },
            None => not_found("not found"),
        };
        let headers = response.headers_mut();
        if custom {
            // An uploaded bundle need not hash its file names, so nothing in
            // it may be cached as immutable.
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        }
        if let Some(builtin) = switch {
            headers.insert(
                header::SET_COOKIE,
                HeaderValue::from_static(if builtin {
                    "gproxy_console=builtin; Path=/; HttpOnly; SameSite=Lax"
                } else {
                    "gproxy_console=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
                }),
            );
        }
        Some(response)
    }

    async fn read_builtin(&self, asset: &str) -> Option<Vec<u8>> {
        match &self.source {
            Source::Disabled => None,
            #[cfg(feature = "embedded-console")]
            Source::Embedded => Embedded::get(asset).map(|file| file.data.into_owned()),
            Source::Directory(root) => {
                // `asset_name` has already rejected `..` and absolute paths, so
                // the join cannot leave the root.
                tokio::fs::read(root.join(asset)).await.ok()
            }
        }
    }
}

/// `Some(true)` for `console=builtin`, `Some(false)` for `console=custom`.
fn requested_switch(query: Option<&str>) -> Option<bool> {
    query?.split('&').find_map(|pair| match pair {
        "console=builtin" => Some(true),
        "console=custom" => Some(false),
        _ => None,
    })
}

fn has_builtin_cookie(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .any(|pair| {
            pair.trim()
                .strip_prefix(BUILTIN_COOKIE)
                .is_some_and(|rest| rest == "=builtin")
        })
}

/// The file `path` asks for, or `None` when the request is not the console's.
///
/// Rejects anything with a `..` segment, a leading `/` or a drive prefix after
/// the prefix is stripped: the directory source joins this onto a root, and a traversal
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
    // `:` too: on Windows `C:/…` or `C:…` is a drive, and joining a drive
    // path onto the root replaces the root. No console asset has one.
    (!rest.split('/').any(|segment| {
        segment.is_empty()
            || segment == ".."
            || segment == "."
            || segment.contains('\\')
            || segment.contains(':')
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

/// The policy every console response carries.
///
/// The bundle is same-origin through and through — no inline script, no
/// third-party origin — so the policy can say exactly that. `style-src` keeps
/// `'unsafe-inline'` for the `style` attributes React renders. `data:` and
/// `blob:` images are generated previews. `frame-ancestors 'none'` (and
/// `X-Frame-Options` for older browsers) keeps the console out of anybody's
/// frame, which matters because a same-site frame would carry the session
/// cookie.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; \
connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; \
frame-ancestors 'none'";

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
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
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
            "/console/C:/Windows/win.ini",
            "/console/C:win.ini",
            "/console/assets/D:/secret",
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
        #[cfg(feature = "embedded-console")]
        let has_bundle = Embedded::get("index.html").is_some();
        #[cfg(not(feature = "embedded-console"))]
        let has_bundle = false;
        assert_eq!(
            console.is_enabled(),
            has_bundle,
            "the console is enabled if and only if a bundle is embedded"
        );

        // The answer when there is nothing to serve, which is the part that
        // must never become a blank page.
        let empty = Console::from_config(&ConsoleConfig {
            enabled: false,
            path: None,
        });
        let response = empty
            .serve(&Method::GET, "/console", None, &HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        // And a non-console path is still nobody's.
        assert!(
            empty
                .serve(&Method::GET, "/v1/messages", None, &HeaderMap::new())
                .await
                .is_none()
        );
    }

    #[test]
    fn every_console_response_refuses_framing_and_sniffing() {
        for asset in ["index.html", "assets/app.js"] {
            let response = asset_response(asset, b"x".to_vec(), false);
            let headers = response.headers();
            let policy = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
            assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
            assert!(policy.contains("script-src 'self';"), "{policy}");
            assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
            assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
        }
    }

    #[tokio::test]
    async fn the_switch_turns_it_off_even_with_a_directory() {
        let console = Console::from_config(&ConsoleConfig {
            enabled: false,
            path: Some("/srv/console".into()),
        });
        assert!(!console.is_enabled());
    }
}
