//! URL assembly helpers shared by channel `prepare_request` impls.

/// Append an extra query string to `url`, picking `?` or `&` as the
/// separator based on whether `url` already contains a query. `extra`
/// must not carry a leading `?`. No-op when `extra` is None or empty.
pub fn append_query(url: &mut String, extra: Option<&str>) {
    let Some(q) = extra else { return };
    if q.is_empty() {
        return;
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    url.push(sep);
    url.push_str(q);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_with_question_mark_when_no_query() {
        let mut url = "https://api.example.com/v1/models".to_string();
        append_query(&mut url, Some("pageSize=10"));
        assert_eq!(url, "https://api.example.com/v1/models?pageSize=10");
    }

    #[test]
    fn appends_with_ampersand_when_query_exists() {
        let mut url = "https://api.example.com/v1/models?key=abc".to_string();
        append_query(&mut url, Some("pageSize=10"));
        assert_eq!(url, "https://api.example.com/v1/models?key=abc&pageSize=10");
    }

    #[test]
    fn noop_for_none_and_empty() {
        let mut url = "https://api.example.com".to_string();
        append_query(&mut url, None);
        append_query(&mut url, Some(""));
        assert_eq!(url, "https://api.example.com");
    }
}
