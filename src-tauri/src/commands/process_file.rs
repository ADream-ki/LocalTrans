use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct ProcessFileOutput {
    pub path: String,
    pub line_count: usize,
    pub char_count: usize,
    pub byte_count: usize,
}

#[tauri::command]
pub fn process_file(input: PathBuf) -> AppResult<ProcessFileOutput> {
    let normalized = dunce::canonicalize(&input)
        .map_err(|_| AppError::InvalidPath(input.display().to_string()))?;
    let content = fs::read_to_string(&normalized)?;
    let normalized_eol = content.replace("\r\n", "\n");

    Ok(ProcessFileOutput {
        path: normalized.display().to_string(),
        line_count: normalized_eol.lines().count(),
        char_count: normalized_eol.chars().count(),
        byte_count: normalized_eol.len(),
    })
}
