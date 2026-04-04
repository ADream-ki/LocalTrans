use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::runtime_adapters::RuntimeAdapterInventory;

#[derive(Debug, Serialize)]
pub struct RuntimeComponentStatus {
    pub ready: bool,
    pub path: String,
    pub message: String,
    pub action: Option<String>,
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPreflightItem {
    pub code: String,
    pub stage: String,
    pub severity: String,
    pub label: String,
    pub message: String,
    pub action: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveRuntimeSummary {
    pub asr_engine: String,
    pub translation_engine: String,
    pub tts_engine: String,
    pub tts_enabled: bool,
    pub tts_auto_play: bool,
    pub bidirectional: bool,
    pub latency_profile: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPreflightStatus {
    pub can_start: bool,
    pub summary: String,
    pub blockers: Vec<SessionPreflightItem>,
    pub warnings: Vec<SessionPreflightItem>,
    pub effective_runtime: EffectiveRuntimeSummary,
    pub workflow_policy: crate::runtime_governance::LociWorkflowPolicySnapshot,
}

#[derive(Debug, Serialize)]
pub struct RuntimeStatus {
    pub models_dir: String,
    pub asr: RuntimeComponentStatus,
    pub translation: RuntimeComponentStatus,
    pub tts: RuntimeComponentStatus,
    pub vad: RuntimeComponentStatus,
    pub tts_engine: String,
    pub loci_unhealthy: bool,
    pub loci_unhealthy_remaining_sec: u32,
    pub loci_workflow_policy: crate::runtime_governance::LociWorkflowPolicySnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    pub log_dir: String,
    pub latest_log: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MtRuntimeCheck {
    pub bundled_python: Option<String>,
    pub bundled_script: Option<String>,
    pub bundled_argos_packages: Option<String>,
    pub package_count: usize,
    pub language_pairs: Vec<String>,
    pub ready: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProfileDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target_user: String,
    pub translation_engine: String,
    pub asr_engine: String,
    pub tts_engine: String,
    pub tts_enabled: bool,
    pub tts_auto_play: bool,
    pub bidirectional: bool,
    pub latency_profile: String,
    pub loci_workflow_plugin: String,
    pub workflows: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyWorkflowProfileRequest {
    pub profile_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyWorkflowProfileResult {
    pub profile: WorkflowProfileDescriptor,
    pub updated_keys: Vec<String>,
    pub preflight: SessionPreflightStatus,
    pub message: String,
}

#[derive(Debug, Clone)]
struct WorkflowProfileSpec {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    target_user: &'static str,
    translation_engine: &'static str,
    asr_engine: &'static str,
    tts_engine: &'static str,
    tts_enabled: bool,
    tts_auto_play: bool,
    bidirectional: bool,
    latency_profile: &'static str,
    loci_workflow_plugin: &'static str,
    workflows: &'static [&'static str],
}

const WORKFLOW_PROFILES: &[WorkflowProfileSpec] = &[
    WorkflowProfileSpec {
        id: "meeting-low-latency",
        name: "低延迟会议模式",
        description: "优先实时性和自动播报，适合同传会议和销售演示。",
        target_user: "sales-demo",
        translation_engine: "loci",
        asr_engine: "whisper",
        tts_engine: "sherpa-melo",
        tts_enabled: true,
        tts_auto_play: true,
        bidirectional: true,
        latency_profile: "low-latency",
        loci_workflow_plugin: "localtrans-meeting-low-latency",
        workflows: &[
            "speech.translate.realtime",
            "speech.asr.whisper",
            "speech.tts.sherpa-melo",
            "speech.latency.low-latency",
            "speech.tts.autoplay",
            "speech.mode.bidirectional",
            "speech.voice.clone",
        ],
    },
    WorkflowProfileSpec {
        id: "privacy-local-only",
        name: "隐私本地模式",
        description: "优先本地化语音链路，避免在线 TTS 依赖。",
        target_user: "private-deployment",
        translation_engine: "loci",
        asr_engine: "whisper",
        tts_engine: "piper",
        tts_enabled: true,
        tts_auto_play: false,
        bidirectional: false,
        latency_profile: "balanced",
        loci_workflow_plugin: "localtrans-privacy-local-only",
        workflows: &[
            "speech.translate.realtime",
            "speech.asr.whisper",
            "speech.tts.piper",
            "speech.latency.balanced",
            "speech.tts.manual",
            "speech.mode.unidirectional",
        ],
    },
    WorkflowProfileSpec {
        id: "caption-high-accuracy",
        name: "高精度字幕模式",
        description: "优先字幕稳定性和准确率，默认关闭自动播报。",
        target_user: "caption-production",
        translation_engine: "loci",
        asr_engine: "sensevoice",
        tts_engine: "piper",
        tts_enabled: false,
        tts_auto_play: false,
        bidirectional: false,
        latency_profile: "high-accuracy",
        loci_workflow_plugin: "localtrans-caption-high-accuracy",
        workflows: &[
            "speech.translate.realtime",
            "speech.asr.sensevoice",
            "speech.tts.none",
            "speech.latency.high-accuracy",
            "speech.mode.unidirectional",
        ],
    },
];

fn to_workflow_profile_descriptor(spec: &WorkflowProfileSpec) -> WorkflowProfileDescriptor {
    WorkflowProfileDescriptor {
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        description: spec.description.to_string(),
        target_user: spec.target_user.to_string(),
        translation_engine: spec.translation_engine.to_string(),
        asr_engine: spec.asr_engine.to_string(),
        tts_engine: spec.tts_engine.to_string(),
        tts_enabled: spec.tts_enabled,
        tts_auto_play: spec.tts_auto_play,
        bidirectional: spec.bidirectional,
        latency_profile: spec.latency_profile.to_string(),
        loci_workflow_plugin: spec.loci_workflow_plugin.to_string(),
        workflows: spec.workflows.iter().map(|v| v.to_string()).collect(),
    }
}

fn build_preflight(
    requested_asr_engine: Option<&str>,
    requested_translation_engine: Option<&str>,
    requested_tts_engine: Option<&str>,
    requested_tts_enabled: Option<bool>,
    requested_tts_auto_play: Option<bool>,
    requested_bidirectional: Option<bool>,
    requested_latency_profile: Option<&str>,
) -> AppResult<SessionPreflightStatus> {
    let selection = crate::runtime_governance::resolve_effective_runtime_selection(
        requested_asr_engine,
        requested_translation_engine,
        requested_tts_engine,
        requested_tts_enabled,
        requested_latency_profile,
        requested_bidirectional,
        requested_tts_auto_play,
        None,
    )?;

    let asr_ready = super::model::has_ready_model("asr")?;
    let loci_ready = super::model::has_ready_model("loci")?;
    let bundled_tts_ready = super::model::has_ready_model("tts")?;
    let mt_ready = check_mt_runtime()?.ready;
    let custom_voice_enabled = crate::commands::config::get_value("customVoiceEnabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let custom_voice_profile_id = crate::commands::config::get_string("customVoiceProfileId");
    let custom_voice_model_path = crate::commands::config::get_string("customVoiceModelPath");

    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if selection.asr_engine == "qwen3-asr" {
        blockers.push(SessionPreflightItem {
            code: "asr.qwen3_unavailable".to_string(),
            stage: "asr".to_string(),
            severity: "blocker".to_string(),
            label: "Qwen3-ASR 尚未接入".to_string(),
            message:
                "当前构建只暴露 qwen3-asr 适配器槽位，尚未包含可执行后端。请切换到 Whisper、SenseVoice 或 Vosk。"
                    .to_string(),
            action: Some("open_settings_page".to_string()),
        });
    } else if !asr_ready {
        blockers.push(SessionPreflightItem {
            code: "asr.model_missing".to_string(),
            stage: "asr".to_string(),
            severity: "blocker".to_string(),
            label: "缺少 ASR 模型".to_string(),
            message: "实时会话需要本地 ASR 模型才能启动。".to_string(),
            action: Some("open_model_page".to_string()),
        });
    }

    if selection.translation_engine == "loci" {
        if !loci_ready {
            blockers.push(SessionPreflightItem {
                code: "translation.loci_model_missing".to_string(),
                stage: "translation".to_string(),
                severity: "blocker".to_string(),
                label: "缺少 Loci 模型".to_string(),
                message: "选择 Loci 翻译时，必须先准备本地 GGUF 模型。".to_string(),
                action: Some("download_loci_model".to_string()),
            });
        }
        if !selection.workflow_policy.policy_active {
            warnings.push(SessionPreflightItem {
                code: "workflow.inactive".to_string(),
                stage: "workflow".to_string(),
                severity: "warning".to_string(),
                label: "Workflow 治理未接管".to_string(),
                message: "当前 Loci 运行时没有激活 workflow rewriter，会退回宿主默认路由选择。"
                    .to_string(),
                action: Some("open_settings_page".to_string()),
            });
        }
        if !selection.workflow_policy.unresolved_workflows.is_empty() {
            warnings.push(SessionPreflightItem {
                code: "workflow.unresolved".to_string(),
                stage: "workflow".to_string(),
                severity: "warning".to_string(),
                label: "存在未识别的 speech workflow 声明".to_string(),
                message: selection.workflow_policy.unresolved_workflows.join(", "),
                action: Some("open_diagnostics_page".to_string()),
            });
        }
    } else if !mt_ready {
        blockers.push(SessionPreflightItem {
            code: "translation.mt_runtime_missing".to_string(),
            stage: "translation".to_string(),
            severity: "blocker".to_string(),
            label: "机翻运行时未就绪".to_string(),
            message: "当前确定性 MT 运行时不完整，无法启动会话。".to_string(),
            action: Some("prepare_mt_runtime".to_string()),
        });
    }

    if selection.tts_enabled {
        match selection.tts_engine.as_str() {
            "qwen3-tts" => blockers.push(SessionPreflightItem {
                code: "tts.qwen3_unavailable".to_string(),
                stage: "tts".to_string(),
                severity: "blocker".to_string(),
                label: "Qwen3-TTS 尚未接入".to_string(),
                message: "当前构建只暴露 qwen3-tts 适配器槽位，尚未包含可执行后端。".to_string(),
                action: Some("open_settings_page".to_string()),
            }),
            "sherpa-melo" | "piper" => {
                if !bundled_tts_ready {
                    blockers.push(SessionPreflightItem {
                        code: "tts.assets_missing".to_string(),
                        stage: "tts".to_string(),
                        severity: "blocker".to_string(),
                        label: "缺少本地 TTS 资源".to_string(),
                        message: "当前 TTS 引擎需要本地语音资源，尚未安装。".to_string(),
                        action: Some("download_tts_model".to_string()),
                    });
                }
            }
            "custom" => {
                let ready = custom_voice_enabled
                    && (custom_voice_profile_id.is_some() || custom_voice_model_path.is_some());
                if !ready {
                    blockers.push(SessionPreflightItem {
                        code: "tts.custom_not_configured".to_string(),
                        stage: "tts".to_string(),
                        severity: "blocker".to_string(),
                        label: "自定义音色未配置".to_string(),
                        message: "当前选择了自定义音色，但没有可用的 profile 或模型路径。"
                            .to_string(),
                        action: Some("open_settings_page".to_string()),
                    });
                }
            }
            "edge-tts" => warnings.push(SessionPreflightItem {
                code: "tts.edge_privacy".to_string(),
                stage: "tts".to_string(),
                severity: "warning".to_string(),
                label: "Edge TTS 会联网".to_string(),
                message: "要合成的文本会发送到微软服务，不适合强隐私场景。".to_string(),
                action: None,
            }),
            _ => {}
        }
    } else {
        warnings.push(SessionPreflightItem {
            code: "tts.disabled".to_string(),
            stage: "tts".to_string(),
            severity: "warning".to_string(),
            label: "自动播报已关闭".to_string(),
            message: "会话可以启动，但不会自动播放翻译语音。".to_string(),
            action: None,
        });
    }

    let can_start = blockers.is_empty();
    let summary = if can_start {
        "当前配置可以启动实时会话".to_string()
    } else {
        format!("启动前还需处理 {} 个阻塞项", blockers.len())
    };

    Ok(SessionPreflightStatus {
        can_start,
        summary,
        blockers,
        warnings,
        effective_runtime: EffectiveRuntimeSummary {
            asr_engine: selection.asr_engine,
            translation_engine: selection.translation_engine,
            tts_engine: selection.tts_engine,
            tts_enabled: selection.tts_enabled,
            tts_auto_play: selection.tts_auto_play,
            bidirectional: selection.bidirectional,
            latency_profile: selection.latency_profile,
        },
        workflow_policy: selection.workflow_policy,
    })
}

#[tauri::command]
pub fn get_runtime_adapter_inventory() -> AppResult<RuntimeAdapterInventory> {
    let selection = crate::runtime_governance::resolve_effective_runtime_selection(
        None, None, None, None, None, None, None, None,
    )?;
    Ok(crate::runtime_adapters::runtime_adapter_inventory(
        &selection.asr_engine,
        &selection.translation_engine,
        &selection.tts_engine,
    ))
}

#[tauri::command]
pub fn get_runtime_status() -> AppResult<RuntimeStatus> {
    let models_dir = super::model::models_dir()?;
    let asr_ready = super::model::has_ready_model("asr")?;
    let loci_ready = super::model::has_ready_model("loci")?;
    let bundled_tts_ready = super::model::has_ready_model("tts")?;
    let selection = crate::runtime_governance::resolve_effective_runtime_selection(
        None, None, None, None, None, None, None, None,
    )?;
    let asr_engine = selection.asr_engine.clone();
    let translation_engine = selection.translation_engine.clone();
    let tts_engine = selection.tts_engine.clone();
    let mt_ready = check_mt_runtime()?.ready;
    let custom_voice_enabled = crate::commands::config::get_value("customVoiceEnabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let custom_voice_profile_id = crate::commands::config::get_string("customVoiceProfileId");
    let custom_voice_model_path = crate::commands::config::get_string("customVoiceModelPath");

    let (translation_ready, translation_path, translation_message, translation_action) =
        if translation_engine == "loci" {
            (
                loci_ready,
                models_dir.join("loci").display().to_string(),
                if loci_ready {
                    "Loci enhanced translation ready".to_string()
                } else {
                    "Loci translation model not installed".to_string()
                },
                if loci_ready {
                    None
                } else {
                    Some("download_loci_model".to_string())
                },
            )
        } else {
            let mt_root = if let Some(path) = resolve_bundled_argos_packages() {
                path
            } else {
                models_dir.join("mt").display().to_string()
            };
            (
                mt_ready,
                mt_root,
                if mt_ready {
                    "Deterministic MT runtime ready".to_string()
                } else {
                    "Bundled MT runtime incomplete".to_string()
                },
                if mt_ready {
                    None
                } else {
                    Some("prepare_mt_runtime".to_string())
                },
            )
        };

    let (tts_ready, tts_path, tts_message, tts_action) = if !selection.tts_enabled {
        (
            true,
            "host://tts-disabled".to_string(),
            "TTS is currently disabled by workflow/config governance".to_string(),
            None,
        )
    } else {
        match tts_engine.as_str() {
        "edge-tts" => (
            true,
            "remote://edge-tts".to_string(),
            "Edge TTS is available as an online backend".to_string(),
            None,
        ),
        "system" => (
            true,
            "host://system-tts".to_string(),
            "System TTS route is available on the host".to_string(),
            None,
        ),
        "custom" => {
            let ready = custom_voice_enabled
                && (custom_voice_profile_id.is_some() || custom_voice_model_path.is_some());
            (
                ready,
                custom_voice_model_path
                    .or(custom_voice_profile_id.map(|id| format!("profile://{id}")))
                    .unwrap_or_else(|| models_dir.join("tts").display().to_string()),
                if ready {
                    "Custom voice route is configured".to_string()
                } else {
                    "Custom voice engine selected, but no profile/model is configured".to_string()
                },
                if ready {
                    None
                } else {
                    Some("open_settings_page".to_string())
                },
            )
        }
        "qwen3-tts" => (
            false,
            models_dir.join("tts").display().to_string(),
            "Qwen3-TTS adapter slot exists but this build does not yet include a concrete backend".to_string(),
            Some("build_or_plugin_required".to_string()),
        ),
        "sherpa-melo" | "piper" => (
            bundled_tts_ready,
            models_dir.join("tts").display().to_string(),
            if bundled_tts_ready {
                "Bundled local TTS assets are ready".to_string()
            } else {
                "Bundled local TTS assets are not installed".to_string()
            },
            if bundled_tts_ready {
                None
            } else {
                Some("download_tts_model".to_string())
            },
        ),
        other => (
            false,
            models_dir.join("tts").display().to_string(),
            format!("Unknown TTS engine: {other}"),
            None,
        ),
    }
    };

    Ok(RuntimeStatus {
        models_dir: models_dir.display().to_string(),
        asr: RuntimeComponentStatus {
            ready: asr_ready,
            path: models_dir.join("asr").display().to_string(),
            message: if asr_ready {
                "ASR model ready".to_string()
            } else {
                "ASR model not installed".to_string()
            },
            action: Some("open_model_page".to_string()),
            engine: Some(asr_engine),
        },
        translation: RuntimeComponentStatus {
            ready: translation_ready,
            path: translation_path,
            message: translation_message,
            action: translation_action,
            engine: Some(translation_engine),
        },
        tts: RuntimeComponentStatus {
            ready: tts_ready,
            path: tts_path,
            message: tts_message,
            action: tts_action,
            engine: Some(tts_engine.clone()),
        },
        vad: RuntimeComponentStatus {
            ready: false,
            path: models_dir.join("vad").display().to_string(),
            message: "Optional VAD model not installed".to_string(),
            action: Some("download_optional_vad".to_string()),
            engine: None,
        },
        tts_engine,
        loci_unhealthy: false,
        loci_unhealthy_remaining_sec: 0,
        loci_workflow_policy: selection.workflow_policy,
    })
}

#[tauri::command]
pub fn get_session_preflight() -> AppResult<SessionPreflightStatus> {
    build_preflight(None, None, None, None, None, None, None)
}

#[tauri::command]
pub fn open_url(url: String) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| crate::error::AppError::Io(format!("Failed to open URL: {e}")))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| crate::error::AppError::Io(format!("Failed to open URL: {e}")))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| crate::error::AppError::Io(format!("Failed to open URL: {e}")))?;
    }

    Ok(())
}

#[tauri::command]
pub fn get_log_status() -> AppResult<LogStatus> {
    let cwd = std::env::current_dir()?;
    let log_dir = cwd.join("logs");
    let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
    if log_dir.exists() {
        for entry in fs::read_dir(&log_dir)? {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            match &latest {
                Some((old, _)) if *old >= modified => {}
                _ => latest = Some((modified, path)),
            }
        }
    }
    Ok(LogStatus {
        log_dir: log_dir.display().to_string(),
        latest_log: latest.map(|(_, p)| p.display().to_string()),
        exists: log_dir.exists(),
    })
}

#[tauri::command]
pub fn check_mt_runtime() -> AppResult<MtRuntimeCheck> {
    let bundled_python = resolve_bundled_python();
    let bundled_script = resolve_bundled_mt_script();
    let bundled_argos_packages = resolve_bundled_argos_packages();
    let (package_count, language_pairs) = match bundled_argos_packages.as_deref() {
        Some(path) => scan_argos_packages(Path::new(path)),
        None => (0, Vec::new()),
    };
    let ready = bundled_python.is_some()
        && bundled_script.is_some()
        && bundled_argos_packages.is_some()
        && package_count > 0;

    let message = if ready {
        format!("Bundled MT runtime ready ({} packages).", package_count)
    } else {
        "Bundled MT runtime incomplete. Run tools/prepare_mt_runtime.ps1 before packaging."
            .to_string()
    };

    Ok(MtRuntimeCheck {
        bundled_python,
        bundled_script,
        bundled_argos_packages,
        package_count,
        language_pairs,
        ready,
        message,
    })
}

#[tauri::command]
pub fn list_workflow_profiles() -> AppResult<Vec<WorkflowProfileDescriptor>> {
    Ok(WORKFLOW_PROFILES
        .iter()
        .map(to_workflow_profile_descriptor)
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub fn apply_workflow_profile(
    request: ApplyWorkflowProfileRequest,
) -> AppResult<ApplyWorkflowProfileResult> {
    let profile_id = request.profile_id.trim().to_ascii_lowercase();
    let Some(spec) = WORKFLOW_PROFILES.iter().find(|item| item.id == profile_id) else {
        return Err(crate::error::AppError::InvalidState(format!(
            "unknown workflow profile id: {profile_id}"
        )));
    };

    let mut updated_keys = Vec::new();
    let mut set_value = |key: &str, value: Value| -> AppResult<()> {
        super::config::set_config_value(key.to_string(), value)?;
        updated_keys.push(key.to_string());
        Ok(())
    };

    set_value(
        "translationEngine",
        Value::String(spec.translation_engine.to_string()),
    )?;
    set_value("asrEngine", Value::String(spec.asr_engine.to_string()))?;
    set_value("ttsEngine", Value::String(spec.tts_engine.to_string()))?;
    set_value("ttsEnabled", Value::Bool(spec.tts_enabled))?;
    set_value("ttsAutoPlay", Value::Bool(spec.tts_auto_play))?;
    set_value(
        "lociWorkflowPlugin",
        Value::String(spec.loci_workflow_plugin.to_string()),
    )?;
    set_value("bidirectional", Value::Bool(spec.bidirectional))?;
    set_value(
        "latencyProfile",
        Value::String(spec.latency_profile.to_string()),
    )?;

    let preflight = build_preflight(
        Some(spec.asr_engine),
        Some(spec.translation_engine),
        Some(spec.tts_engine),
        Some(spec.tts_enabled),
        Some(spec.tts_auto_play),
        Some(spec.bidirectional),
        Some(spec.latency_profile),
    )?;

    Ok(ApplyWorkflowProfileResult {
        profile: to_workflow_profile_descriptor(spec),
        updated_keys,
        preflight,
        message: format!(
            "Applied workflow profile '{}' with plugin '{}'",
            spec.id, spec.loci_workflow_plugin
        ),
    })
}

fn resolve_bundled_python() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir
            .join("resources")
            .join("mt-runtime")
            .join("python")
            .join("python.exe"),
        exe_dir.join("mt-runtime").join("python").join("python.exe"),
        exe_dir.join("python").join("python.exe"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| path_to_string(p.as_path()))
}

fn resolve_bundled_argos_packages() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir
            .join("resources")
            .join("mt-runtime")
            .join("argos-packages"),
        exe_dir.join("mt-runtime").join("argos-packages"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| path_to_string(p.as_path()))
}

fn resolve_bundled_mt_script() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir
            .join("resources")
            .join("mt-runtime")
            .join("mt_translate.py"),
        exe_dir.join("mt-runtime").join("mt_translate.py"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| path_to_string(p.as_path()))
}

fn scan_argos_packages(root: &Path) -> (usize, Vec<String>) {
    let mut count = 0usize;
    let mut pairs = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(v) => v,
        Err(_) => return (0, pairs),
    };
    for entry in entries.flatten() {
        let meta = entry.path().join("metadata.json");
        if !meta.exists() {
            continue;
        }
        let text = match fs::read_to_string(&meta) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let json: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let from = json.get("from_code").and_then(Value::as_str).unwrap_or("");
        let to = json.get("to_code").and_then(Value::as_str).unwrap_or("");
        if !from.is_empty() && !to.is_empty() {
            pairs.push(format!("{from}->{to}"));
        }
        count += 1;
    }
    pairs.sort();
    pairs.dedup();
    (count, pairs)
}

fn path_to_string(path: &Path) -> String {
    path.display().to_string()
}
