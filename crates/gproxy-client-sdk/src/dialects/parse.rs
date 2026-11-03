use super::*;
use crate::dialects;
pub(crate) fn parse(bytes: &[u8], request: &GenerateRequest) -> Result<GenerateResult> {
    match request.model.dialect {
        #[cfg(feature = "claude")]
        Dialect::Claude => serde_json::from_slice(bytes)
            .map_err(Error::invalid)
            .and_then(dialects::claude::parse),
        #[cfg(feature = "openai")]
        Dialect::Openai => serde_json::from_slice(bytes)
            .map_err(Error::invalid)
            .and_then(dialects::openai::parse),
        #[cfg(feature = "gemini")]
        Dialect::Gemini => serde_json::from_slice(bytes)
            .map_err(Error::invalid)
            .and_then(dialects::gemini::parse),
        #[cfg(feature = "chat")]
        Dialect::OpenaiChat => serde_json::from_slice(bytes)
            .map_err(Error::invalid)
            .and_then(|response| {
                dialects::chat::parse(response, dialects::chat::output_format(request)?)
            }),
    }
}
