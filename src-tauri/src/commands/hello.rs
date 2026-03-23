use serde::Serialize;

use crate::error::AppResult;

#[derive(Debug, Serialize)]
pub struct HelloOutput {
    pub message: String,
}

#[tauri::command]
pub fn hello(name: String) -> AppResult<HelloOutput> {
    Ok(HelloOutput {
        message: format!("Hello, {name}!"),
    })
}
