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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredLociRewriterTarget {
    pub component: String,
    pub plugin_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LociGovernanceSnapshot {
    pub translation_engine: String,
    pub loci_selected: bool,
    pub runtime_ready: bool,
    pub status_message: String,
    pub default_model_dir: String,
    pub model_path: Option<String>,
    pub plugin_dirs: Vec<String>,
    pub configured_rewriter_targets: Vec<ConfiguredLociRewriterTarget>,
    pub active_rewriter_inventory: Vec<CoreRewriterInventoryStatus>,
}

pub type LociWorkflowPolicySnapshot = crate::runtime_governance::LociWorkflowPolicySnapshot;

fn configured_plugin_dirs_snapshot() -> Vec<String> {
    #[cfg(feature = "loci-backend")]
    {
        return crate::loci_runtime::configured_plugin_dirs()
            .into_iter()
            .map(|path| path.display().to_string())
            .collect();
    }

    #[cfg(not(feature = "loci-backend"))]
    {
        let Some(value) = crate::commands::config::get_value("lociPluginDirs") else {
            return Vec::new();
        };

        match value {
            serde_json::Value::Array(items) => items
                .into_iter()
                .filter_map(|item| item.as_str().map(str::trim).map(ToString::to_string))
                .filter(|item| !item.is_empty())
                .collect(),
            serde_json::Value::String(raw) => raw
                .split(['\n', ';'])
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect(),
            _ => Vec::new(),
        }
    }
}

fn configured_rewriter_targets_snapshot() -> Vec<ConfiguredLociRewriterTarget> {
    const REWRITER_KEYS: [(&str, &str); 7] = [
        ("lociInferencePlugin", "inference"),
        ("lociModelPlugin", "model"),
        ("lociHardwarePlugin", "hardware"),
        ("lociWorkflowPlugin", "workflow"),
        ("lociEventBusPlugin", "event_bus"),
        ("lociPluginManagerPlugin", "plugin_manager"),
        ("lociUiHostPlugin", "ui_host"),
    ];

    REWRITER_KEYS
        .into_iter()
        .filter_map(|(key, component)| {
            crate::commands::config::get_string(key)
                .map(|plugin_name| plugin_name.trim().to_string())
                .filter(|plugin_name| !plugin_name.is_empty())
                .map(|plugin_name| ConfiguredLociRewriterTarget {
                    component: component.to_string(),
                    plugin_name,
                })
        })
        .collect()
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
    match value
        .unwrap_or("directory")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
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
pub fn get_loci_runtime_snapshot(
    request: Option<LociSnapshotRequest>,
) -> AppResult<LociSnapshotResponse> {
    #[cfg(feature = "loci-backend")]
    {
        let model_path =
            resolve_model_path(request.as_ref().and_then(|req| req.model_path.as_deref()))?;
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
        let model_path =
            resolve_model_path(request.as_ref().and_then(|req| req.model_path.as_deref()))?;
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
pub fn get_loci_governance_snapshot(
    request: Option<LociSnapshotRequest>,
) -> AppResult<LociGovernanceSnapshot> {
    let translation_engine = crate::commands::translation::resolved_translation_engine(None);
    let loci_selected = translation_engine == "loci";
    let model_path = crate::loci_runtime::resolve_loci_model_path(
        request.as_ref().and_then(|req| req.model_path.as_deref()),
    );
    let default_model_dir = crate::loci_runtime::default_loci_dir()
        .display()
        .to_string();
    let configured_rewriter_targets = configured_rewriter_targets_snapshot();
    let configured_plugin_dirs = configured_plugin_dirs_snapshot();

    #[cfg(feature = "loci-backend")]
    {
        if !loci_selected {
            return Ok(LociGovernanceSnapshot {
                translation_engine,
                loci_selected,
                runtime_ready: false,
                status_message:
                    "translationEngine is not set to loci; Loci runtime is currently inactive"
                        .to_string(),
                default_model_dir,
                model_path: model_path.map(|path| path.display().to_string()),
                plugin_dirs: configured_plugin_dirs,
                configured_rewriter_targets,
                active_rewriter_inventory: Vec::new(),
            });
        }

        let Some(model_path) = model_path else {
            return Ok(LociGovernanceSnapshot {
                translation_engine,
                loci_selected,
                runtime_ready: false,
                status_message: "No Loci model is currently resolvable".to_string(),
                default_model_dir,
                model_path: None,
                plugin_dirs: configured_plugin_dirs,
                configured_rewriter_targets,
                active_rewriter_inventory: Vec::new(),
            });
        };

        let active_rewriter_inventory =
            crate::loci_runtime::current_rewriter_inventory(&model_path)
                .map_err(|e| AppError::InvalidState(e.to_string()))?;
        let plugin_dirs = crate::loci_runtime::current_plugin_dirs(&model_path)
            .map_err(|e| AppError::InvalidState(e.to_string()))?;

        return Ok(LociGovernanceSnapshot {
            translation_engine,
            loci_selected,
            runtime_ready: true,
            status_message:
                "Loci runtime is active and governance inventory was resolved successfully"
                    .to_string(),
            default_model_dir,
            model_path: Some(model_path.display().to_string()),
            plugin_dirs,
            configured_rewriter_targets,
            active_rewriter_inventory,
        });
    }

    #[cfg(not(feature = "loci-backend"))]
    {
        Ok(LociGovernanceSnapshot {
            translation_engine,
            loci_selected,
            runtime_ready: false,
            status_message: "loci-backend feature is not enabled in this build".to_string(),
            default_model_dir,
            model_path: model_path.map(|path| path.display().to_string()),
            plugin_dirs: configured_plugin_dirs,
            configured_rewriter_targets,
            active_rewriter_inventory: Vec::new(),
        })
    }
}

#[tauri::command]
pub fn get_loci_workflow_policy(
    request: Option<LociSnapshotRequest>,
) -> AppResult<LociWorkflowPolicySnapshot> {
    crate::runtime_governance::workflow_policy_snapshot(
        None,
        request.as_ref().and_then(|req| req.model_path.as_deref()),
    )
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
