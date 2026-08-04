use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::{CustomToolGrammarSyntax, Extra, JsonSchema};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct FunctionCall {
    pub arguments: String,
    pub name: String,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CustomToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolInputFormat>,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum CustomToolInputFormat {
    Text(CustomToolTextFormat),
    Grammar(CustomToolGrammarFormat),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CustomToolTextFormat {
    #[serde(rename = "type")]
    pub type_: CustomToolTextFormatType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CustomToolTextFormatType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CustomToolGrammarFormat {
    pub type_: CustomToolGrammarFormatType,
    pub grammar: CustomToolGrammar,
}

impl Serialize for CustomToolGrammarFormat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // OpenAI's grammar format is flat on the wire. Keep the nested
        // `grammar` Rust field for API compatibility while emitting the real
        // provider shape: {"type":"grammar","definition":...,"syntax":...}.
        let mut state = serializer.serialize_struct("CustomToolGrammarFormat", 3)?;
        state.serialize_field("type", &self.type_)?;
        state.serialize_field("definition", &self.grammar.definition)?;
        state.serialize_field("syntax", &self.grammar.syntax)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for CustomToolGrammarFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(rename = "type")]
            type_: CustomToolGrammarFormatType,
            definition: String,
            syntax: CustomToolGrammarSyntax,
        }

        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            type_: wire.type_,
            grammar: CustomToolGrammar {
                definition: wire.definition,
                syntax: wire.syntax,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CustomToolGrammarFormatType {
    #[serde(rename = "grammar")]
    Grammar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CustomToolGrammar {
    pub definition: String,
    pub syntax: CustomToolGrammarSyntax,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct NamedTool {
    pub name: String,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn custom_tool_grammar_format_matches_flat_openai_wire_shape() {
        let wire = json!({
            "type": "grammar",
            "definition": "start: /[a-z]+/",
            "syntax": "lark"
        });

        let format: CustomToolInputFormat = serde_json::from_value(wire.clone()).unwrap();
        let CustomToolInputFormat::Grammar(format) = &format else {
            panic!("expected grammar format");
        };
        assert_eq!(format.type_, CustomToolGrammarFormatType::Grammar);
        assert_eq!(format.grammar.definition, "start: /[a-z]+/");
        assert_eq!(format.grammar.syntax, CustomToolGrammarSyntax::Lark);
        assert_eq!(serde_json::to_value(format).unwrap(), wire);
    }
}
