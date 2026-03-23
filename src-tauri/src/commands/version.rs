use serde::Serialize;

use crate::error::AppResult;

#[derive(Debug, Serialize)]
pub struct VersionOutput {
    pub app: String,
    pub version: String,
}

#[tauri::command]
pub fn version() -> AppResult<VersionOutput> {
    Ok(VersionOutput {
        app: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
