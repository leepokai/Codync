//! Helper protocol types: displays and the helper's status.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Size in points.
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub main: bool,
}

/// What the helper reports about itself.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperStatus {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub displays: Vec<Display>,
    /// Screen capture is permitted.
    #[serde(default)]
    pub capture: bool,
    /// Input injection (and the accessibility tree) is permitted.
    #[serde(default)]
    pub input: bool,
}
