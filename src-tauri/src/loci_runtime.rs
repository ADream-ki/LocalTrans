use std::path::PathBuf;

#[cfg(feature = "loci-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "loci-backend")]
use loci::{
    CoreComponent, CoreRewriterActivationRequest, CoreRewriterActivationStatus,
    CoreRewriterInventoryStatus, InferenceEngine, ManagementService, PluginLoadRequest,
    PluginLoadSourceKind, PluginLoadStatus, RuntimeSnapshot, WorkflowInventoryStatus,
};
#[cfg(feature = "loci-backend")]
use std::path::Path;
#[cfg(feature = "loci-backend")]
use std::sync::{Mutex, OnceLock};

const DEFAULT_TRANSLATION_ENGINE: &str = "nllb";

#[cfg(feature = "loci-backend")]
struct ManagedLociRuntime {
    model_path: PathBuf,
    plugin_dirs: Vec<PathBuf>,
    service: ManagementService,
}

#[cfg(feature = "loci-backend")]
fn runtime_state() -> &'static Mutex<Option<ManagedLociRuntime>> {
    static STATE: OnceLock<Mutex<Option<ManagedLociRuntime>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

pub fn configured_translation_engine() -> String {
    crate::commands::config::get_string("translationEngine")
        .unwrap_or_else(|| DEFAULT_TRANSLATION_ENGINE.to_string())
}

pub fn normalize_translation_engine(engine: &str) -> String {
    match engine.trim().to_ascii_lowercase().as_str() {
        "loci" => "loci".to_string(),
        "nllb" | "argos" | "mt" | "m2m" => "nllb".to_string(),
        other => other.to_string(),
    }
}

pub fn configured_loci_model_path() -> Option<PathBuf> {
    crate::commands::config::get_string("lociModelPath")
        .map(|value| PathBuf::from(value.trim()))
        .filter(|path| !path.as_os_str().is_empty())
}

pub fn default_loci_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("models")
        .join("loci")
}

pub fn find_default_loci_model() -> Option<PathBuf> {
    let dir = default_loci_dir();
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gguf"))
            != Some(true)
        {
            continue;
        }
        let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
        match &best {
            Some((best_size, _)) if *best_size >= size => {}
            _ => best = Some((size, path)),
        }
    }
    best.map(|(_, path)| path)
}

pub fn resolve_loci_model_path(requested: Option<&str>) -> Option<PathBuf> {
    requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(configured_loci_model_path)
        .or_else(find_default_loci_model)
}

#[cfg(feature = "loci-backend")]
fn configured_plugin_dirs() -> Vec<PathBuf> {
    let Some(value) = crate::commands::config::get_value("lociPluginDirs") else {
        return Vec::new();
    };

    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::trim).map(ToString::to_string))
            .filter(|item| !item.is_empty())
            .map(PathBuf::from)
            .collect(),
        serde_json::Value::String(raw) => raw
            .split(['\n', ';'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(PathBuf::from)
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(feature = "loci-backend")]
fn configured_rewriter_targets() -> Vec<(CoreComponent, String)> {
    const REWRITER_KEYS: [(&str, CoreComponent); 7] = [
        ("lociInferencePlugin", CoreComponent::Inference),
        ("lociModelPlugin", CoreComponent::Model),
        ("lociHardwarePlugin", CoreComponent::Hardware),
        ("lociWorkflowPlugin", CoreComponent::Workflow),
        ("lociEventBusPlugin", CoreComponent::EventBus),
        ("lociPluginManagerPlugin", CoreComponent::PluginManager),
        ("lociUiHostPlugin", CoreComponent::UiHost),
    ];

    REWRITER_KEYS
        .into_iter()
        .filter_map(|(key, component)| {
            crate::commands::config::get_string(key)
                .map(|plugin| plugin.trim().to_string())
                .filter(|plugin| !plugin.is_empty())
                .map(|plugin| (component, plugin))
        })
        .collect()
}

#[cfg(feature = "loci-backend")]
fn build_service(model_path: &Path) -> Result<ManagedLociRuntime> {
    let engine = InferenceEngine::builder()
        .with_backend_name("llama.cpp")
        .with_model_path(model_path)
        .build()
        .context("failed to build Loci runtime engine")?;

    let service = ManagementService::new(engine);
    let plugin_dirs = configured_plugin_dirs();

    for plugin_dir in &plugin_dirs {
        if plugin_dir.exists() {
            let request = PluginLoadRequest {
                path: plugin_dir.display().to_string(),
                source_kind: PluginLoadSourceKind::Directory,
            };
            service.load_plugins(request).with_context(|| {
                format!("failed to load Loci plugins from {}", plugin_dir.display())
            })?;
        }
    }

    for (component, plugin_name) in configured_rewriter_targets() {
        service
            .activate_core_rewriter(CoreRewriterActivationRequest {
                component,
                plugin_name,
            })
            .context("failed to activate configured Loci core rewriter")?;
    }

    Ok(ManagedLociRuntime {
        model_path: model_path.to_path_buf(),
        plugin_dirs,
        service,
    })
}

#[cfg(feature = "loci-backend")]
pub fn ensure_management_service(model_path: &Path) -> Result<ManagementService> {
    let mut state = runtime_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("Loci runtime state lock poisoned"))?;

    let needs_reload = match state.as_ref() {
        Some(runtime) => runtime.model_path != model_path,
        None => true,
    };

    if needs_reload {
        *state = Some(build_service(model_path)?);
    }

    Ok(state.as_ref().expect("runtime initialized").service.clone())
}

#[cfg(feature = "loci-backend")]
pub fn current_runtime_snapshot(model_path: &Path) -> Result<RuntimeSnapshot> {
    ensure_management_service(model_path)?
        .runtime_snapshot()
        .map_err(Into::into)
}

#[cfg(feature = "loci-backend")]
pub fn current_rewriter_inventory(model_path: &Path) -> Result<Vec<CoreRewriterInventoryStatus>> {
    ensure_management_service(model_path)?
        .core_rewriter_inventory()
        .map_err(Into::into)
}

#[cfg(feature = "loci-backend")]
pub fn current_workflow_inventory(model_path: &Path) -> Result<WorkflowInventoryStatus> {
    ensure_management_service(model_path)?
        .workflow_inventory()
        .map_err(Into::into)
}

#[cfg(feature = "loci-backend")]
pub fn load_plugins(model_path: &Path, request: PluginLoadRequest) -> Result<PluginLoadStatus> {
    ensure_management_service(model_path)?
        .load_plugins(request)
        .map_err(Into::into)
}

#[cfg(feature = "loci-backend")]
pub fn activate_rewriter(
    model_path: &Path,
    request: CoreRewriterActivationRequest,
) -> Result<CoreRewriterActivationStatus> {
    ensure_management_service(model_path)?
        .activate_core_rewriter(request)
        .map_err(Into::into)
}

#[cfg(feature = "loci-backend")]
pub fn current_plugin_dirs(model_path: &Path) -> Result<Vec<String>> {
    let _ = ensure_management_service(model_path)?;
    let state = runtime_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("Loci runtime state lock poisoned"))?;
    Ok(state
        .as_ref()
        .map(|runtime| {
            runtime
                .plugin_dirs
                .iter()
                .map(|path| path.display().to_string())
                .collect()
        })
        .unwrap_or_default())
}
