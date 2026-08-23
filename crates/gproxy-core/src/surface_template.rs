use crate::error::CoreError;

pub(crate) fn render(
    template: &str,
    params: &[(&'static str, String)],
) -> Result<String, CoreError> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        rendered.push_str(&rest[..open]);
        let placeholder = &rest[open + 1..];
        let close = placeholder
            .find('}')
            .ok_or_else(|| CoreError::Internal("surface upstream template is malformed".into()))?;
        let name = &placeholder[..close];
        let value = params
            .iter()
            .find_map(|(param, value)| (*param == name).then_some(value))
            .ok_or_else(|| {
                CoreError::Internal("surface upstream template has an unknown parameter".into())
            })?;
        rendered.push_str(value);
        rest = &placeholder[close + 1..];
    }
    if rest.contains('}') {
        return Err(CoreError::Internal(
            "surface upstream template is malformed".into(),
        ));
    }
    rendered.push_str(rest);
    Ok(rendered)
}
