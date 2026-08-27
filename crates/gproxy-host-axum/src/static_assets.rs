use bytes::Bytes;
use http::request::Parts;
use http::{HeaderValue, Method, Response, StatusCode};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/web"]
#[exclude = ".gitkeep"]
struct WebAssets;

pub(crate) fn serve(parts: &Parts) -> Option<Response<Bytes>> {
    if parts.method != Method::GET && parts.method != Method::HEAD {
        return None;
    }
    let request_path = parts.uri.path();
    if request_path == "/build-info.js" {
        return Some(build_info(parts.method == Method::HEAD));
    }
    let asset = if matches!(
        request_path,
        "/" | "/admin" | "/admin/" | "/portal" | "/portal/"
    ) {
        "index.html"
    } else if let Some(path) = request_path.strip_prefix('/')
        && (path.starts_with("assets/") || path == "favicon.svg")
    {
        path
    } else {
        return None;
    };
    if WebAssets::get("index.html").is_none() {
        return Some(text(
            StatusCode::NOT_FOUND,
            "web assets are not embedded; run `pnpm build` in console/ and rebuild gproxy",
        ));
    }
    let Some(content) = WebAssets::get(asset) else {
        return Some(text(StatusCode::NOT_FOUND, "not found"));
    };
    let mut response = Response::new(if parts.method == Method::HEAD {
        Bytes::new()
    } else if asset == "index.html" {
        Bytes::from(
            String::from_utf8_lossy(&content.data).replace(
                "</head>",
                "<script src=\"/build-info.js\"></script><script src=\"/announcements.js\"></script></head>",
            ),
        )
    } else {
        Bytes::from(content.data.into_owned())
    });
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_str(
            mime_guess::from_path(asset)
                .first_raw()
                .unwrap_or("application/octet-stream"),
        )
        .expect("MIME types are valid header values"),
    );
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        if asset == "index.html" {
            HeaderValue::from_static("no-cache")
        } else {
            HeaderValue::from_static("public, max-age=31536000, immutable")
        },
    );
    Some(response)
}

fn build_info(head: bool) -> Response<Bytes> {
    let value = serde_json::json!({
        "version": crate::BUILD_VERSION,
        "channel": crate::BUILD_CHANNEL,
        "buildHash": crate::BUILD_HASH,
        "installationKind": crate::INSTALLATION_KIND,
    });
    let body = format!("globalThis.__GPROXY_BUILD_INFO__ = {value};\n");
    let mut response = Response::new(if head {
        Bytes::new()
    } else {
        Bytes::from(body)
    });
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}

fn text(status: StatusCode, body: &'static str) -> Response<Bytes> {
    let mut response = Response::new(Bytes::from_static(body.as_bytes()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
