use serde::Serialize;

#[cfg(feature = "loci-backend")]
use anyhow::Context;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LociWorkflowPolicySnapshot {
    pub translation_engine: String,
    pub governance_enabled: bool,
    pub runtime_ready: bool,
    pub policy_active: bool,
    pub active_workflow_rewriter: Option<String>,
    pub workflows: Vec<String>,
    pub effective_asr_engine: Option<String>,
    pub effective_tts_engine: Option<String>,
    pub effective_tts_enabled: Option<bool>,
    pub preferred_latency_profile: Option<String>,
    pub force_bidirectional: Option<bool>,
    pub force_tts_auto_play: Option<bool>,
    pub supports_streaming: bool,
    pub supports_voice_cloning: bool,
    pub unresolved_workflows: Vec<String>,
    pub status_message: String,
}

#[derive(Debug, Clone)]
pub struct EffectiveRuntimeSelection {
    pub asr_engine: String,
    pub translation_engine: String,
    pub tts_engine: String,
    pub tts_enabled: bool,
    pub tts_auto_play: bool,
    pub bidirectional: bool,
    pub latency_profile: Option<String>,
    pub workflow_policy: LociWorkflowPolicySnapshot,
}

pub(crate) fn resolved_asr_engine(requested: Option<&str>) -> String {
    requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| {
            crate::commands::config::get_string("asrEngine").map(|value| value.to_ascii_lowercase())
        })
        .unwrap_or_else(|| "whisper".to_string())
}

pub(crate) fn resolved_tts_engine(requested: Option<&str>) -> String {
    requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| {
            crate::commands::config::get_string("ttsEngine").map(|value| value.to_ascii_lowercase())
        })
        .unwrap_or_else(|| "sherpa-melo".to_string())
}

pub(crate) fn resolved_tts_enabled(requested: Option<bool>) -> bool {
    requested
        .or_else(|| {
            crate::commands::config::get_value("ttsEnabled").and_then(|value| value.as_bool())
        })
        .unwrap_or(true)
}

pub(crate) fn resolved_tts_auto_play(requested: Option<bool>) -> bool {
    requested
        .or_else(|| {
            crate::commands::config::get_value("ttsAutoPlay").and_then(|value| value.as_bool())
        })
        .unwrap_or(true)
}

pub fn workflow_policy_snapshot(
    requested_translation_engine: Option<&str>,
    requested_model_path: Option<&str>,
) -> crate::error::AppResult<LociWorkflowPolicySnapshot> {
    #[cfg(feature = "loci-backend")]
    {
        let translation_engine =
            crate::commands::translation::resolved_translation_engine(requested_translation_engine);

        if translation_engine != "loci" {
            return Ok(LociWorkflowPolicySnapshot {
                translation_engine,
                governance_enabled: false,
                runtime_ready: false,
                policy_active: false,
                active_workflow_rewriter: None,
                workflows: Vec::new(),
                effective_asr_engine: None,
                effective_tts_engine: None,
                effective_tts_enabled: None,
                preferred_latency_profile: None,
                force_bidirectional: None,
                force_tts_auto_play: None,
                supports_streaming: false,
                supports_voice_cloning: false,
                unresolved_workflows: Vec::new(),
                status_message:
                    "translationEngine is not set to loci; workflow governance is inactive"
                        .to_string(),
            });
        }

        let Some(model_path) = crate::loci_runtime::resolve_loci_model_path(requested_model_path)
        else {
            return Ok(LociWorkflowPolicySnapshot {
                translation_engine,
                governance_enabled: true,
                runtime_ready: false,
                policy_active: false,
                active_workflow_rewriter: None,
                workflows: Vec::new(),
                effective_asr_engine: None,
                effective_tts_engine: None,
                effective_tts_enabled: None,
                preferred_latency_profile: None,
                force_bidirectional: None,
                force_tts_auto_play: None,
                supports_streaming: false,
                supports_voice_cloning: false,
                unresolved_workflows: Vec::new(),
                status_message: "No Loci model is currently resolvable".to_string(),
            });
        };

        let inventory = match crate::loci_runtime::current_workflow_inventory(&model_path)
            .with_context(|| {
                format!(
                    "failed to resolve Loci workflow inventory ({})",
                    model_path.display()
                )
            }) {
            Ok(inventory) => inventory,
            Err(error) => {
                return Ok(LociWorkflowPolicySnapshot {
                    translation_engine,
                    governance_enabled: true,
                    runtime_ready: false,
                    policy_active: false,
                    active_workflow_rewriter: None,
                    workflows: Vec::new(),
                    effective_asr_engine: None,
                    effective_tts_engine: None,
                    effective_tts_enabled: None,
                    preferred_latency_profile: None,
                    force_bidirectional: None,
                    force_tts_auto_play: None,
                    supports_streaming: false,
                    supports_voice_cloning: false,
                    unresolved_workflows: Vec::new(),
                    status_message: error.to_string(),
                });
            }
        };

        let mut snapshot = LociWorkflowPolicySnapshot {
            translation_engine,
            governance_enabled: true,
            runtime_ready: true,
            policy_active: inventory.active_workflow_rewriter.is_some(),
            active_workflow_rewriter: inventory.active_workflow_rewriter,
            workflows: inventory.workflows,
            effective_asr_engine: None,
            effective_tts_engine: None,
            effective_tts_enabled: None,
            preferred_latency_profile: None,
            force_bidirectional: None,
            force_tts_auto_play: None,
            supports_streaming: false,
            supports_voice_cloning: false,
            unresolved_workflows: Vec::new(),
            status_message: String::new(),
        };

        apply_workflow_declarations(&mut snapshot);

        snapshot.status_message = if !snapshot.policy_active {
            "No active Loci workflow rewriter is configured; host runtime stays on local route selection"
                .to_string()
        } else if snapshot.workflows.is_empty() {
            "Active workflow rewriter is configured, but it declares no workflows".to_string()
        } else {
            format!(
                "Workflow governance is active via {}; host runtime routes are derived from declared speech workflows",
                snapshot
                    .active_workflow_rewriter
                    .as_deref()
                    .unwrap_or("unknown-workflow-rewriter")
            )
        };

        return Ok(snapshot);
    }

    #[cfg(not(feature = "loci-backend"))]
    {
        let _ = requested_model_path;
        Ok(LociWorkflowPolicySnapshot {
            translation_engine: crate::commands::translation::resolved_translation_engine(
                requested_translation_engine,
            ),
            governance_enabled: false,
            runtime_ready: false,
            policy_active: false,
            active_workflow_rewriter: None,
            workflows: Vec::new(),
            effective_asr_engine: None,
            effective_tts_engine: None,
            effective_tts_enabled: None,
            preferred_latency_profile: None,
            force_bidirectional: None,
            force_tts_auto_play: None,
            supports_streaming: false,
            supports_voice_cloning: false,
            unresolved_workflows: Vec::new(),
            status_message: "loci-backend feature is not enabled in this build".to_string(),
        })
    }
}

pub fn resolve_effective_runtime_selection(
    requested_asr_engine: Option<&str>,
    requested_translation_engine: Option<&str>,
    requested_tts_engine: Option<&str>,
    requested_tts_enabled: Option<bool>,
    requested_latency_profile: Option<&str>,
    requested_bidirectional: Option<bool>,
    requested_tts_auto_play: Option<bool>,
    requested_model_path: Option<&str>,
) -> crate::error::AppResult<EffectiveRuntimeSelection> {
    let translation_engine =
        crate::commands::translation::resolved_translation_engine(requested_translation_engine);
    let mut selection = EffectiveRuntimeSelection {
        asr_engine: resolved_asr_engine(requested_asr_engine),
        translation_engine: translation_engine.clone(),
        tts_engine: resolved_tts_engine(requested_tts_engine),
        tts_enabled: resolved_tts_enabled(requested_tts_enabled),
        tts_auto_play: resolved_tts_auto_play(requested_tts_auto_play),
        bidirectional: requested_bidirectional.unwrap_or(false),
        latency_profile: requested_latency_profile
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        workflow_policy: workflow_policy_snapshot(Some(&translation_engine), requested_model_path)?,
    };

    if selection.workflow_policy.policy_active {
        if let Some(engine) = selection.workflow_policy.effective_asr_engine.as_deref() {
            selection.asr_engine = engine.to_string();
        }
        if let Some(engine) = selection.workflow_policy.effective_tts_engine.as_deref() {
            selection.tts_engine = engine.to_string();
        }
        if let Some(enabled) = selection.workflow_policy.effective_tts_enabled {
            selection.tts_enabled = enabled;
        }
        if let Some(profile) = selection
            .workflow_policy
            .preferred_latency_profile
            .as_deref()
        {
            selection.latency_profile = Some(profile.to_string());
        }
        if let Some(bidirectional) = selection.workflow_policy.force_bidirectional {
            selection.bidirectional = bidirectional;
        }
        if let Some(tts_auto_play) = selection.workflow_policy.force_tts_auto_play {
            selection.tts_auto_play = tts_auto_play;
        }
    }

    Ok(selection)
}

#[cfg(feature = "loci-backend")]
fn apply_workflow_declarations(snapshot: &mut LociWorkflowPolicySnapshot) {
    for workflow in &snapshot.workflows {
        let normalized = workflow.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }

        match normalized.as_str() {
            "speech.translate.realtime" | "speech.pipeline.realtime" | "speech.realtime" => {
                snapshot.supports_streaming = true;
            }
            "speech.mode.bidirectional" => {
                snapshot.force_bidirectional = Some(true);
            }
            "speech.mode.unidirectional" => {
                snapshot.force_bidirectional = Some(false);
            }
            "speech.tts.autoplay" => {
                snapshot.force_tts_auto_play = Some(true);
            }
            "speech.tts.manual" | "speech.tts.on-demand" => {
                snapshot.force_tts_auto_play = Some(false);
            }
            "speech.tts.none" | "speech.tts.disabled" => {
                snapshot.effective_tts_enabled = Some(false);
            }
            "speech.voice.clone" | "speech.voice.zero-shot-clone" | "speech.voice.icl" => {
                snapshot.supports_voice_cloning = true;
            }
            _ => {
                if let Some(engine) = normalized.strip_prefix("speech.asr.") {
                    if let Some(engine) = normalize_asr_engine(engine) {
                        snapshot.effective_asr_engine = Some(engine.to_string());
                    } else {
                        snapshot.unresolved_workflows.push(workflow.clone());
                    }
                    continue;
                }
                if let Some(engine) = normalized.strip_prefix("speech.tts.") {
                    if let Some(engine) = normalize_tts_engine(engine) {
                        snapshot.effective_tts_enabled = Some(true);
                        snapshot.effective_tts_engine = Some(engine.to_string());
                    } else if !matches!(
                        engine,
                        "autoplay" | "manual" | "on-demand" | "none" | "disabled"
                    ) {
                        snapshot.unresolved_workflows.push(workflow.clone());
                    }
                    continue;
                }
                if let Some(profile) = normalized.strip_prefix("speech.latency.") {
                    if let Some(profile) = normalize_latency_profile(profile) {
                        snapshot.preferred_latency_profile = Some(profile.to_string());
                    } else {
                        snapshot.unresolved_workflows.push(workflow.clone());
                    }
                    continue;
                }
                if normalized.starts_with("speech.") {
                    snapshot.unresolved_workflows.push(workflow.clone());
                }
            }
        }
    }
}

#[cfg(feature = "loci-backend")]
fn normalize_asr_engine(value: &str) -> Option<&'static str> {
    match value {
        "whisper" => Some("whisper"),
        "sensevoice" => Some("sensevoice"),
        "vosk" => Some("vosk"),
        "qwen3" | "qwen3-asr" => Some("qwen3-asr"),
        _ => None,
    }
}

#[cfg(feature = "loci-backend")]
fn normalize_tts_engine(value: &str) -> Option<&'static str> {
    match value {
        "sherpa" | "sherpa-melo" | "melo" => Some("sherpa-melo"),
        "edge" | "edge-tts" => Some("edge-tts"),
        "system" => Some("system"),
        "piper" => Some("piper"),
        "custom" => Some("custom"),
        "qwen3" | "qwen3-tts" => Some("qwen3-tts"),
        _ => None,
    }
}

#[cfg(feature = "loci-backend")]
fn normalize_latency_profile(value: &str) -> Option<&'static str> {
    match value {
        "low" | "low-latency" | "low_latency" | "realtime" => Some("low-latency"),
        "balanced" | "balance" | "default" => Some("balanced"),
        "high" | "high-accuracy" | "high_accuracy" | "accuracy" => Some("high-accuracy"),
        _ => None,
    }
}
