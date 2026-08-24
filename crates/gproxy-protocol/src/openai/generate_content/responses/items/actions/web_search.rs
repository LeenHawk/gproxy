use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum WebSearchAction {
    #[serde(rename = "search")]
    Search {
        #[serde(skip_serializing_if = "Option::is_none")]
        queries: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sources: Option<Vec<WebSearchSource>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "open_page")]
    OpenPage {
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "find_in_page")]
    FindInPage {
        pattern: String,
        url: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchSource {
    #[serde(rename = "type")]
    pub type_: WebSearchSourceType,
    pub url: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum WebSearchSourceType {
    #[serde(rename = "url")]
    Url,
}
