use gproxy_core::RoutingMode;

pub fn normalize_path(path: &str) -> (RoutingMode, String) {
    if is_api_path(path) {
        return (RoutingMode::Aggregated, path.to_owned());
    }
    let Some((name, remainder)) = path.strip_prefix('/').and_then(|path| path.split_once('/'))
    else {
        return (RoutingMode::Aggregated, path.to_owned());
    };
    let remainder = format!("/{remainder}");
    if name.is_empty() || !is_api_path(&remainder) {
        return (RoutingMode::Aggregated, path.to_owned());
    }
    (
        RoutingMode::Named {
            name: name.to_owned(),
        },
        remainder,
    )
}

fn is_api_path(path: &str) -> bool {
    path == "/v1" || path.starts_with("/v1/") || path == "/v1beta" || path.starts_with("/v1beta/")
}
