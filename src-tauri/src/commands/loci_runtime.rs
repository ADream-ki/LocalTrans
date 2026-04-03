use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[cfg(feature = "loci-backend")]
use loci::{
    CoreComponent, CoreRewriterActivationRequest, CoreRewriterActivationStatus,
    CoreRewriterInventoryStatus, PluginLoadRequest, PluginLoadSourceKind, PluginLoadStatus,
    RuntimeSnapshot,
};

#[cfg(not(feature = "loci-backend"))]
#[derive(Debug, Clone, Serialize)]
pub struct CoreRewriterInventoryStatus {
    pub component: String,
    pub active_plugin_name: Option<String>,
    pub available_plugins: Vec<String>,
}

#[cfg(not(feature = "loci-backend"))]
#[derive(Debug, Clone, Serialize)]
pub struct PluginLoadStatus {
    pub status: &'static str,
    pub path: String,
    pub source_kind: String,
    pub loaded_count: usize,
    pub loaded_plugin_names: Vec<String>,
    pub plugin_count_after: usize,
    pub active_inference: Option<String>,
}

#[cfg(not(feature = "loci-backend"))]
#[derive(Debug, Clone, Serialize)]
pub struct CoreRewriterActivationStatus {
    pub status: &'static str,
    pub component: String,
    pub plugin_name: String,
    pub active_inference: Option<String>,
}

#[cfg(not(feature = "loci-backend"))]
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSnapshot {
    pub plugin_count: usize,
    pub loaded_plugin_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LociSnapshotRequest {
    pub model_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LociSnapshotResponse {
    #[cfg(feature = "loci-backend")]
    pub snapshot: RuntimeSnapshot,
    pub model_path: String,
    pub plugin_dirs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadLociPluginsRequest {
    pub model_path: Option<String>,
    pub path: String,
    pub source_kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateLociRewriterRequest {
    pub model_path: Option<String>,
    pub component: String,
    pub plugin_name: String,
}

#[cfg_attr(not(feature = "loci-backend"), allow(dead_code))]
fn resolve_model_path(input: Option<&str>) -> AppResult<std::path::PathBuf> {
    crate::loci_runtime::resolve_loci_model_path(input).ok_or_else(|| {
        AppError::InvalidState(format!(
            "Loci model not found under {}",
            crate::loci_runtime::default_loci_dir().display()
        ))
    })
}

#[cfg(feature = "loci-backend")]
fn parse_source_kind(value: Option<&str>) -> PluginLoadSourceKind {
    match value.unwrap_or("directory").trim().to_ascii_lowercase().as_str() {
        "bundle" | "bundle_file" => PluginLoadSourceKind::BundleFile,
        _ => PluginLoadSourceKind::Directory,
    }
}

#[cfg(feature = "loci-backend")]
fn parse_component(value: &str) -> AppResult<CoreComponent> {
    match value.trim().to_ascii_lowercase().as_str() {
        "inference" => Ok(CoreComponent::Inference),
        "model" => Ok(CoreComponent::Model),
        "hardware" => Ok(CoreComponent::Hardware),
        "workflow" => Ok(CoreComponent::Workflow),
        "event_bus" | "eventbus" => Ok(CoreComponent::EventBus),
        "plugin_manager" | "pluginmanager" => Ok(CoreComponent::PluginManager),
        "ui_host" | "uihost" => Ok(CoreComponent::UiHost),
        other => Err(AppError::InvalidState(format!(
            "unsupported Loci core component: {other}"
        ))),
    }
}

#[tauri::command]
pub fn get_loci_runtime_snapshot(request: Option<LociSnapshotRequest>) -> AppResult<LociSnapshotResponse> {
    #[cfg(feature = "loci-backend")]
    {
        let model_path = resolve_model_path(request.as_ref().and_then(|req| req.model_path.as_deref()))?;
        let snapshot = crate::loci_runtime::current_runtime_snapshot(&model_path)
            .map_err(|e| AppError::InvalidState(e.to_string()))?;
        let plugin_dirs = crate::loci_runtime::current_plugin_dirs(&model_path)
            .map_err(|e| AppError::InvalidState(e.to_string()))?;
        return Ok(LociSnapshotResponse {
            snapshot,
            model_path: model_path.display().to_string(),
            plugin_dirs,
        });
    }

    #[cfg(not(feature = "loci-backend"))]
    {
        let _ = request;
        Err(AppError::InvalidState(
            "loci-backend feature is not enabled".to_string(),
        ))
    }
}

#[tauri::command]
pub fn get_loci_rewriter_inventory(
    request: Option<LociSnapshotRequest>,
) -> AppResult<Vec<CoreRewriterInventoryStatus>> {
    #[cfg(feature = "loci-backend")]
    {
        let model_path = resolve_model_path(request.as_ref().and_then(|req| req.model_path.as_deref()))?;
        return crate::loci_runtime::current_rewriter_inventory(&model_path)
            .map_err(|e| AppError::InvalidState(e.to_string()));
    }

    #[cfg(not(feature = "loci-backend"))]
    {
        let _ = request;
        Err(AppError::InvalidState(
            "loci-backend feature is not enabled".to_string(),
        ))
    }
}

#[tauri::command]
pub fn load_loci_plugins(request: LoadLociPluginsRequest) -> AppResult<PluginLoadStatus> {
    #[cfg(feature = "loci-backend")]
    {
        let model_path = resolve_model_path(request.model_path.as_deref())?;
        return crate::loci_runtime::load_plugins(
            &model_path,
            PluginLoadRequest {
                path: request.path,
                source_kind: parse_source_kind(request.source_kind.as_deref()),
            },
        )
        .map_err(|e| AppError::InvalidState(e.to_string()));
    }

    #[cfg(not(feature = "loci-backend"))]
    {
        let _ = request;
        Err(AppError::InvalidState(
            "loci-backend feature is not enabled".to_string(),
        ))
    }
}

#[tauri::command]
pub fn activate_loci_rewriter(
    request: ActivateLociRewriterRequest,
) -> AppResult<CoreRewriterActivationStatus> {
    #[cfg(feature = "loci-backend")]
    {
        let model_path = resolve_model_path(request.model_path.as_deref())?;
        let component = parse_component(&request.component)?;
        return crate::loci_runtime::activate_rewriter(
            &model_path,
            CoreRewriterActivationRequest {
                component,
                plugin_name: request.plugin_name,
            },
        )
        .map_err(|e| AppError::InvalidState(e.to_string()));
    }

    #[cfg(not(feature = "loci-backend"))]
    {
        let _ = request;
        Err(AppError::InvalidState(
            "loci-backend feature is not enabled".to_string(),
        ))
    }
}
