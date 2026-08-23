mod code;
mod functions;

fn correlated(
    explicit: Option<String>,
    pending: Option<&mut std::collections::VecDeque<String>>,
) -> Option<String> {
    match explicit {
        Some(id) => {
            if let Some(pending) = pending
                && let Some(index) = pending.iter().position(|candidate| candidate == &id)
            {
                pending.remove(index);
            }
            Some(id)
        }
        None => pending.and_then(|pending| pending.pop_front()),
    }
}
