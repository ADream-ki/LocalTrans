use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::error::AppResult;

fn config_file() -> AppResult<PathBuf> {
    Ok(std::env::current_dir()?.join(".localtrans-config.json"))
}

fn load_map() -> AppResult<BTreeMap<String, Value>> {
    let path = config_file()?;
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(path)?;
    let parsed = serde_json::from_str::<BTreeMap<String, Value>>(&text)
        .unwrap_or_else(|_| BTreeMap::new());
    Ok(parsed)
}

fn save_map(map: &BTreeMap<String, Value>) -> AppResult<()> {
    let path = config_file()?;
    let text = serde_json::to_string_pretty(map).map_err(|e| crate::error::AppError::Io(e.to_string()))?;
    fs::write(path, text)?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_app_config(config: Value) -> AppResult<()> {
    let map = match config {
        Value::Object(obj) => obj.into_iter().collect::<BTreeMap<_, _>>(),
        _ => BTreeMap::new(),
    };
    save_map(&map)
}

#[tauri::command]
pub fn get_app_config() -> AppResult<Value> {
    let map = load_map()?;
    Ok(Value::Object(map.into_iter().collect()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_config_value(key: String, value: Value) -> AppResult<()> {
    let mut map = load_map()?;
    map.insert(key, value);
    save_map(&map)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_config_value(key: String) -> AppResult<Value> {
    let map = load_map()?;
    Ok(map.get(&key).cloned().unwrap_or(Value::Null))
}
