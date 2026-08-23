use http::Method;

use crate::spec::{Matched, PathPattern, Seg};

/// Linear scan over every operation's ingress table. The table is small
/// and static; a hot-path matcher can replace the scan later without the
/// signature changing.
pub fn match_ingress(method: &Method, path: &str) -> Option<Matched> {
    let path = path.strip_prefix('/')?;

    for (operation, spec) in &crate::specs::REGISTRY {
        for ingress in spec.ingress {
            if ingress.method != method {
                continue;
            }
            if let Some(params) = match_path_segments(ingress.pattern, path) {
                return Some(Matched {
                    operation: *operation,
                    kind: ingress.kind,
                    stream: ingress.stream,
                    upgrade: ingress.upgrade,
                    params,
                });
            }
        }
    }

    None
}

/// Match one declared path pattern and return its captures.
pub fn match_path(pattern: PathPattern, path: &str) -> Option<Vec<(&'static str, String)>> {
    match_path_segments(pattern, path.strip_prefix('/')?)
}

fn match_path_segments(pattern: PathPattern, path: &str) -> Option<Vec<(&'static str, String)>> {
    let mut segments = path.split('/');
    let mut params = Vec::new();

    for pattern_segment in pattern.0 {
        let segment = segments.next()?;
        match pattern_segment {
            Seg::Lit(expected) if segment == *expected => {}
            Seg::Lit(_) => return None,
            Seg::Param(name) if !segment.is_empty() => params.push((*name, segment.to_owned())),
            Seg::Param(_) => return None,
            Seg::ParamAction(name, action) => {
                let value = segment.strip_suffix(action)?.strip_suffix(':')?;
                if value.is_empty() {
                    return None;
                }
                params.push((*name, value.to_owned()));
            }
            Seg::Rest(name) => {
                if segment.is_empty() {
                    return None;
                }
                let mut rest = segment.to_owned();
                for segment in segments {
                    if segment.is_empty() {
                        return None;
                    }
                    rest.push('/');
                    rest.push_str(segment);
                }
                params.push((*name, rest));
                return Some(params);
            }
        }
    }

    segments.next().is_none().then_some(params)
}
