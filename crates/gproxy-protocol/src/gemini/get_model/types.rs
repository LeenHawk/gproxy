use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// Format: models/{model}. It takes the form models/{model}.
    pub name: String,
    /// The name of the base model, pass this to the generation request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model_id: Option<String>,
    /// The major version (e.g., 1.0 or 1.5).
    pub version: String,
    /// The name can be up to 128 characters long and can consist of any UTF-8 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Maximum number of input tokens allowed for this model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_limit: Option<u32>,
    /// Maximum number of output tokens available for this model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_token_limit: Option<u32>,
    /// The corresponding API method names are defined as Pascal case strings, such as generateMessage and generateContent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_generation_methods: Option<Vec<String>>,
    /// Whether the model supports thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    /// Values can range over [0.0, max_temperature], inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// The maximum temperature this model can use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_temperature: Option<f64>,
    /// Nucleus sampling value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Top-k sampling value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}
