use crate::gemini::generate_content::request::{GenerateContentPath, GenerateContentRequestBody};

#[derive(Debug, Clone)]
pub struct StreamGenerateContentRequest {
    pub path: GenerateContentPath,
    pub body: GenerateContentRequestBody,
}
