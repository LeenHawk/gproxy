use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SafetyCheck {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ComputerAction {
    #[serde(rename = "click")]
    Click {
        button: ComputerMouseButton,
        x: f64,
        y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "double_click")]
    DoubleClick {
        keys: Vec<String>,
        x: f64,
        y: f64,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "drag")]
    Drag {
        path: Vec<ComputerCoordinate>,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "keypress")]
    Keypress {
        keys: Vec<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "move")]
    Move {
        x: f64,
        y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "screenshot")]
    Screenshot {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "scroll")]
    Scroll {
        scroll_x: f64,
        scroll_y: f64,
        x: f64,
        y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "type")]
    Type {
        text: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "wait")]
    Wait {
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ComputerMouseButton {
    Left,
    Right,
    Wheel,
    Back,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerCoordinate {
    pub x: f64,
    pub y: f64,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerScreenshot {
    #[serde(rename = "type")]
    pub type_: ComputerScreenshotType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ComputerScreenshotType {
    #[serde(rename = "computer_screenshot")]
    ComputerScreenshot,
}
