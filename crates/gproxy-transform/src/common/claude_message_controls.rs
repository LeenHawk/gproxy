use gproxy_protocol::claude;

pub(crate) fn apply(
    messages: &mut Vec<claude::MessageParam>,
    output_config: &mut Option<claude::OutputConfig>,
) {
    if let Some(last_user) = messages.iter().rposition(is_user)
        && let Some(effort) = messages[..last_user]
            .iter()
            .rev()
            .filter(is_system)
            .find_map(|message| {
                message
                    .output_config
                    .as_ref()
                    .and_then(|config| config.effort.clone())
            })
    {
        output_config
            .get_or_insert_with(|| {
                crate::wire!(claude::OutputConfig {
                    effort: None,
                    format: None,
                    task_budget: None,
                    rest: Default::default(),
                })
            })
            .effort = Some(effort);
    }

    let mut later_user = vec![false; messages.len()];
    let mut seen_user = false;
    for (index, message) in messages.iter().enumerate().rev() {
        later_user[index] = seen_user;
        seen_user |= is_user(message);
    }
    let mut index = 0;
    messages.retain(|message| {
        let cleared = is_system(&message)
            && later_user[index]
            && matches!(
                message.clear_at,
                Some(claude::MessageClearAt::Known(
                    claude::MessageClearAtKnown::NextUserMessage
                ))
            );
        let effort_only = is_system(&message)
            && message.output_config.is_some()
            && matches!(&message.content, claude::StringOrArray::Array(blocks) if blocks.is_empty());
        index += 1;
        !cleared && !effort_only
    });
}

fn is_user(message: &claude::MessageParam) -> bool {
    matches!(
        message.role,
        claude::MessageRole::Known(claude::MessageRoleKnown::User)
    )
}

fn is_system(message: &&claude::MessageParam) -> bool {
    matches!(
        message.role,
        claude::MessageRole::Known(claude::MessageRoleKnown::System)
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn renders_turn_scoped_messages_and_applies_effort_for_current_user() {
        let mut request: claude::CreateMessageRequestBody = serde_json::from_value(json!({
            "model": "claude-fable-5-1",
            "max_tokens": 1024,
            "output_config": {"effort": "high"},
            "messages": [
                {"role": "user", "content": "first"},
                {"role": "system", "clear_at": "next_user_message", "content": "old"},
                {"role": "assistant", "content": "done"},
                {"role": "system", "content": [], "output_config": {"effort": "low"}},
                {"role": "user", "content": "current"},
                {"role": "system", "clear_at": "next_user_message", "content": "live"}
            ]
        }))
        .unwrap();

        apply(&mut request.messages, &mut request.output_config);

        let wire = serde_json::to_value(request).unwrap();
        assert_eq!(wire["output_config"]["effort"], "low");
        assert_eq!(wire["messages"].as_array().unwrap().len(), 4);
        assert!(
            wire["messages"].as_array().unwrap().iter().all(|message| {
                message["content"] != "old" && message["output_config"].is_null()
            })
        );
        assert_eq!(wire["messages"][3]["content"], "live");
    }
}
