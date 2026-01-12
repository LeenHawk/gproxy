use std::collections::HashMap;

use url::Url;

pub fn apply_gemini_query(url: &mut Url, query: &HashMap<String, String>, add_alt_sse: bool) {
    if query.is_empty() && !add_alt_sse {
        return;
    }
    let mut pairs = url.query_pairs_mut();
    for (key, value) in query {
        if key == "key" {
            continue;
        }
        if add_alt_sse && key == "alt" {
            continue;
        }
        pairs.append_pair(key, value);
    }
    if add_alt_sse {
        pairs.append_pair("alt", "sse");
    }
}
