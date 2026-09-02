pub(crate) fn normalize(input: &str) -> Option<String> {
    let mut text = input.trim();
    if let Some((name, value)) = text.split_once(':')
        && name.trim().eq_ignore_ascii_case("cookie")
    {
        text = value.trim();
    }
    let session_key = text.split(';').find_map(|part| {
        part.trim()
            .strip_prefix("sessionKey=")
            .map(str::trim)
            .filter(|value| value.starts_with("sk-ant-sid"))
    });
    let session_key = session_key.or_else(|| {
        (text.starts_with("sk-ant-sid") && !text.contains(['=', ';'])).then_some(text)
    })?;
    if !text.contains("sessionKey=") {
        return Some(format!("sessionKey={session_key}"));
    }
    let pairs = text
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty() && part.contains('='))
        .collect::<Vec<_>>();
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn accepts_full_headers_and_bare_session_keys() {
        assert_eq!(
            normalize("Cookie: cf_clearance=clear; sessionKey=sk-ant-sid01-example; __cf_bm=bm")
                .as_deref(),
            Some("cf_clearance=clear; sessionKey=sk-ant-sid01-example; __cf_bm=bm")
        );
        assert_eq!(
            normalize("sk-ant-sid02-example").as_deref(),
            Some("sessionKey=sk-ant-sid02-example")
        );
    }
}
