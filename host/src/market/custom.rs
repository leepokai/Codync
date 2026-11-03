//! Custom connector installation.

use super::install::custom;
use super::{add_connector, connectors};
use anyhow::{Result, anyhow};
use serde_json::Value;

/// A connector you describe yourself: a command line or an https URL.
pub async fn add_custom_connector(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let name =
        b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`name` is required"))?;
    let all = connectors(store)?;
    let c = custom(name, b, &all).await?;
    add_connector(store, c)
}
