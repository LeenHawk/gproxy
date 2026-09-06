pub fn valid_oauth_redirect(value: &str) -> bool {
    let Ok(uri) = value.parse::<http::Uri>() else {
        return false;
    };
    !value.contains('#')
        && uri
            .authority()
            .is_some_and(|authority| !authority.as_str().contains('@'))
        && (uri.scheme_str() == Some("https")
            || (uri.scheme_str() == Some("http")
                && matches!(uri.host(), Some("127.0.0.1" | "[::1]" | "localhost"))))
}

pub fn oauth_redirect_allowed(registered: &[String], candidate: &str) -> bool {
    if !valid_oauth_redirect(candidate) {
        return false;
    }
    let Ok(candidate_uri) = candidate.parse::<http::Uri>() else {
        return false;
    };
    registered.iter().any(|entry| {
        if entry == candidate {
            return true;
        }
        let Ok(uri) = entry.parse::<http::Uri>() else {
            return false;
        };
        uri.scheme_str() == Some("http")
            && matches!(uri.host(), Some("127.0.0.1" | "[::1]"))
            && uri.port_u16().is_none()
            && candidate_uri.scheme() == uri.scheme()
            && candidate_uri.host() == uri.host()
            && candidate_uri.path_and_query() == uri.path_and_query()
    })
}
