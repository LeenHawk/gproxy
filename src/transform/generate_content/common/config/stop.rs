use crate::protocol::openai;

pub(in crate::transform::generate_content) fn openai_stop_to_vec(
    stop: Option<openai::StringOrList>,
) -> Option<Vec<String>> {
    match stop? {
        openai::StringOrList::String(value) => Some(vec![value]),
        openai::StringOrList::List(values) => Some(values),
    }
}

pub(in crate::transform::generate_content) fn vec_to_openai_stop(
    stop: Option<Vec<String>>,
) -> Option<openai::StringOrList> {
    let mut values = stop?;
    if values.is_empty() {
        None
    } else if values.len() == 1 {
        Some(openai::StringOrList::String(values.remove(0)))
    } else {
        Some(openai::StringOrList::List(values))
    }
}
