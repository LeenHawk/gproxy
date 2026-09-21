//! Console and portal sessions on the wire: the cookie, and the two ways a
//! management request proves who it is.
//!
//! v3 had two cookies, `gproxy_admin_session` and `gproxy_portal_session`,
//! because it had two session tables. v4 has one `user_sessions` row per
//! person and one [`Caller`] shape, so it has one cookie. Which surface a
//! person may reach is a property of their role, checked per request, not of
//! which cookie they were handed.
//!
//! # Two credentials, one ladder
//!
//! A management request authenticates by **session cookie first, bearer token
//! second**. The cookie is what a browser sends and the token is what a script
//! sends, and the order matters only for a browser that also carries an
//! `Authorization` header — in which case the cookie is the credential the
//! person is actually signed in with.
//!
//! The consequence is the CSRF rule. A cookie is attached by the browser to
//! **any** request that reaches this origin, including one a foreign page
//! caused. A bearer token is not: somebody had to put it in a header. So an
//! unsafe method authenticated by a cookie is same-origin checked
//! ([`verify_same_origin`]), and one authenticated by a token is not — running
//! the check against API-key callers would break every non-browser client for
//! no gain.

use axum::response::Response;
use gproxy_app::{App, AppError, Caller, CallerKind, auth::verify_same_origin};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, Method, header};

/// The one session cookie this host sets.
pub const COOKIE_NAME: &str = "gproxy_session";

/// Authenticate a management request and check it is not a cross-origin
/// cookie write.
///
/// It takes the method and the headers rather than the request, for a
/// mechanical reason worth writing down: `axum::body::Body` is `Send` but not
/// `Sync`, so a `&Request` held across an `await` makes the enclosing future
/// non-`Send` and no middleware built on it compiles. Nothing here reads a
/// body anyway.
///
/// The `Caller` is returned rather than stored, so a handler takes it as an
/// argument and cannot forget to look for it.
pub async fn authenticate<C>(
    app: &App<C>,
    method: &Method,
    headers: &HeaderMap,
) -> Result<Caller, AppError>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let data = app.data();
    let authenticator = app.authenticator(&data);
    let caller = match cookie(headers, COOKIE_NAME) {
        Some(token) => {
            authenticator
                .authenticate_session(token, crate::now_ms())
                .await?
        }
        None => authenticator.authenticate_request(headers).await?,
    };
    if caller.kind == CallerKind::Session {
        verify_same_origin(method, headers, &app.config().cors_origins)?;
    }
    Ok(caller)
}

/// The value of one cookie, or `None`.
///
/// A hand-rolled split rather than a cookie crate: this reads exactly one
/// name, and `Cookie` is a `;`-separated list of `name=value` pairs with
/// optional spaces. Values are taken verbatim — a session token is base64url,
/// which needs no decoding — and a repeated name takes the first, as browsers
/// send the most specific path first.
pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

/// The `Set-Cookie` that opens a session.
///
/// `HttpOnly` so a script cannot read it, `SameSite=Lax` so a normal
/// navigation to the console still carries it while a cross-site `POST` does
/// not, `Path=/` because one cookie now serves both surfaces, and `Secure`
/// only when the request arrived over TLS — a development instance on
/// `http://127.0.0.1` would otherwise be handed a cookie the browser refuses
/// to store.
pub fn set_cookie(token: &str, ttl_secs: u64, secure: bool) -> Option<HeaderValue> {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={ttl_secs}{secure}"
    ))
    .ok()
}

/// The `Set-Cookie` that ends one. `Max-Age=0` with the same attributes, which
/// is what makes a browser drop the pair rather than keep a second one.
pub fn clear_cookie(secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure}"
    ))
    .expect("a fixed cookie string is a valid header value")
}

/// Attach a `Set-Cookie` to a response.
pub fn with_cookie(mut response: Response, value: HeaderValue) -> Response {
    response.headers_mut().append(header::SET_COOKIE, value);
    response
}

/// The audit action a management route records, derived from the route rather
/// than repeated at every handler.
///
/// `matched` is axum's matched path — `/admin/api/api-keys/{id}/rotate` — so
/// the name is a property of the route table and a new route cannot forget to
/// name itself. The rule: the family is the first segment after the surface
/// prefix, and the action is the last literal segment, or the method's verb
/// when the route ends in a parameter.
pub fn audit_action(surface: &str, matched: &str, method: &Method) -> String {
    let mut segments = matched
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        // `/admin/api/…` and `/portal/api/…`: drop the two prefix segments.
        .skip(2);
    let family = segments.next().unwrap_or("root").replace('-', "_");
    let tail = segments.filter(|segment| !is_parameter(segment)).last();
    let verb = match tail {
        Some(tail) => tail.replace('-', "_"),
        None => verb_of(method, matched).to_owned(),
    };
    format!("{surface}.{family}.{verb}")
}

fn is_parameter(segment: &str) -> bool {
    segment.starts_with('{') && segment.ends_with('}')
}

/// The CRUD verb a method means on a route that names no action of its own.
fn verb_of(method: &Method, matched: &str) -> &'static str {
    let addresses_one = matched.ends_with('}');
    match *method {
        Method::POST => "create",
        Method::PUT | Method::PATCH => "update",
        Method::DELETE => "delete",
        _ if addresses_one => "get",
        _ => "list",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn one_cookie_is_read_out_of_the_list() {
        let map = headers(&[("cookie", "other=1; gproxy_session=abc123; last=2")]);
        assert_eq!(cookie(&map, COOKIE_NAME), Some("abc123"));
        // Several `Cookie` headers is legal over HTTP/2.
        let map = headers(&[("cookie", "other=1"), ("cookie", "gproxy_session=xyz")]);
        assert_eq!(cookie(&map, COOKIE_NAME), Some("xyz"));
        // A prefix of the name is not the name.
        let map = headers(&[("cookie", "gproxy_session_old=abc")]);
        assert_eq!(cookie(&map, COOKIE_NAME), None);
        // An empty value is no cookie.
        let map = headers(&[("cookie", "gproxy_session=")]);
        assert_eq!(cookie(&map, COOKIE_NAME), None);
        assert_eq!(cookie(&HeaderMap::new(), COOKIE_NAME), None);
    }

    #[test]
    fn the_cookie_is_httponly_and_only_secure_over_tls() {
        let value = set_cookie("tok", 3600, true).unwrap();
        let value = value.to_str().unwrap();
        assert!(value.starts_with("gproxy_session=tok;"));
        assert!(value.contains("HttpOnly"));
        assert!(value.contains("SameSite=Lax"));
        assert!(value.contains("Path=/"));
        assert!(value.contains("Max-Age=3600"));
        assert!(value.contains("Secure"));
        // A plain-HTTP development instance must still be able to sign in.
        let plain = set_cookie("tok", 60, false).unwrap();
        assert!(!plain.to_str().unwrap().contains("Secure"));
        // Clearing repeats the attributes so the browser drops the same pair.
        let cleared = clear_cookie(false);
        let cleared = cleared.to_str().unwrap();
        assert!(cleared.contains("Max-Age=0"));
        assert!(cleared.contains("Path=/"));
    }

    #[test]
    fn an_action_name_comes_out_of_the_route_table() {
        assert_eq!(
            audit_action("admin", "/admin/api/users", &Method::POST),
            "admin.users.create"
        );
        assert_eq!(
            audit_action("admin", "/admin/api/users/{id}", &Method::PATCH),
            "admin.users.update"
        );
        assert_eq!(
            audit_action("admin", "/admin/api/users/{id}", &Method::DELETE),
            "admin.users.delete"
        );
        assert_eq!(
            audit_action("admin", "/admin/api/users/{id}", &Method::GET),
            "admin.users.get"
        );
        assert_eq!(
            audit_action("admin", "/admin/api/users", &Method::GET),
            "admin.users.list"
        );
        // A named action wins over the method's verb, and a dashed path
        // segment becomes the underscored name the trail uses.
        assert_eq!(
            audit_action("admin", "/admin/api/api-keys/{id}/rotate", &Method::POST),
            "admin.api_keys.rotate"
        );
        assert_eq!(
            audit_action("portal", "/portal/api/keys/{id}", &Method::DELETE),
            "portal.keys.delete"
        );
        assert_eq!(
            audit_action("portal", "/portal/api/login", &Method::POST),
            "portal.login.create"
        );
    }
}
