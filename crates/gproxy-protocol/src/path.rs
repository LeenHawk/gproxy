use http::Method;

use crate::operation::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};
use crate::spec::{Matched, PathPattern, Seg};

/// Linear scan over every operation's ingress table. The table is small
/// and static; a hot-path matcher can replace the scan later without the
/// signature changing.
pub fn match_ingress(method: &Method, path: &str) -> Option<Matched> {
    match_ingress_for(method, path, None)
}

/// Match with a preferred family when identical vendor paths overlap. The
/// first canonical row remains the default for callers without wire-profile
/// evidence; the engine derives a preference from protocol headers.
pub fn match_ingress_for(
    method: &Method,
    path: &str,
    preferred: Option<WireFamily>,
) -> Option<Matched> {
    let path = path.strip_prefix('/')?;
    let mut fallback = None;

    for (operation, spec) in &crate::specs::REGISTRY {
        for ingress in spec.ingress {
            if ingress.method != method {
                continue;
            }
            if let Some(params) = match_path_segments(ingress.pattern, path) {
                let matched = Matched {
                    operation: *operation,
                    kind: ingress.kind,
                    stream: ingress.stream,
                    framing: ingress.framing,
                    upgrade: ingress.upgrade,
                    params,
                };
                if preferred.is_some_and(|family| ingress.kind == OperationKind::Family(family)) {
                    return Some(matched);
                }
                fallback.get_or_insert(matched);
            }
        }
    }

    fallback
}

/// Match one declared path pattern and return its captures.
pub fn match_path(pattern: PathPattern, path: &str) -> Option<Vec<(&'static str, String)>> {
    match_path_segments(pattern, path.strip_prefix('/')?)
}

/// Canonical upstream method/path for a native operation key. Targets derive
/// from the ingress registry so transforms do not carry a second path table.
pub fn request_target(key: OperationKey, model: &str) -> Option<(Method, String)> {
    let operation = if key.operation() == Operation::StreamGenerateContent
        && key.kind()
            != OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
    {
        Operation::GenerateContent
    } else {
        key.operation()
    };
    let ingress = operation
        .spec()
        .ingress
        .iter()
        .find(|ingress| ingress.kind == key.kind())?;
    let mut path = String::new();
    for segment in ingress.pattern.0 {
        path.push('/');
        match segment {
            Seg::Lit(value) => path.push_str(value),
            Seg::Param("id" | "model") => path.push_str(&encode_segment(model)?),
            Seg::Param(_) | Seg::Rest(_) => return None,
            Seg::ParamAction("model", action) => {
                path.push_str(&encode_segment(model)?);
                path.push(':');
                path.push_str(action);
            }
            Seg::ParamAction(_, _) => return None,
        }
    }
    Some((ingress.method.clone(), path))
}

fn encode_segment(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    Some(encoded)
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
