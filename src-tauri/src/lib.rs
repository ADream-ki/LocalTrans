//! LocalTrans - Real-time Audio Transcription and Translation
//!
//! This is the main library entry point for the Tauri application.
//! It initializes all components and registers Tauri commands.

pub mod commands;
pub mod audio;
pub mod logging;
pub mod asr;
pub mod translation;
mod pipeline;
pub mod tts;
pub mod session_bus;

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::process::Command;
use parking_lot::Mutex as SyncMutex;
use tauri::Manager;
use pipeline::RealtimePipeline;
use crate::audio::AudioCapture;
use crate::commands::translation::TranslationService;
use crate::tts::playback::PlaybackControl;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Initialize shared state for the realtime pipeline
            // Using Arc<RealtimePipeline> for thread-safe shared access
            app.manage(SyncMutex::<Option<Arc<RealtimePipeline>>>::new(None));

            // Audio capture state used by diagnostics and mic tests
            app.manage(StdMutex::<Option<AudioCapture>>::new(None));

            // Translation service for diagnostics (cached engine)
            app.manage(Arc::new(SyncMutex::new(TranslationService::default())));

            // Playback control for cancellable TTS
            app.manage(StdMutex::new(PlaybackControl::default()));
            
            // Initialize logging
            logging::init_logging();

            tracing::info!(
                log_dir = %logging::default_log_dir().display(),
                "application setup complete"
            );

            // Defer worker bootstrap until the GUI event loop is alive.
            // This avoids startup contention with WebView2 initialization on some Windows hosts.
            maybe_autostart_cli_worker();

            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            if std::env::var("LOCALTRANS_FORCE_SHOW_WINDOW")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            
            tracing::info!("LocalTrans application starting...");
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Audio commands
            commands::audio::get_audio_devices,
            commands::audio::start_capture,
            commands::audio::stop_capture,
            
            // Session/Pipeline commands
            commands::session::start_session,
            commands::session::stop_session,
            commands::session::pause_session,
            commands::session::resume_session,
            commands::session::get_session_status,
            commands::session::get_session_stats,
            commands::session::get_session_history,
            commands::session::clear_session_history,
            commands::session::export_history,
            commands::session::update_languages,
            
             // Model commands
             commands::model::list_models,
             commands::model::get_models_dir,
             commands::model::get_runtime_status,
             commands::model::download_model,
             commands::model::delete_model,
             commands::model::get_log_status,

            // Translation commands
            commands::translation::translate_text,
            
            // TTS commands
            commands::tts::get_tts_voices,
            commands::tts::get_tts_output_devices,
            commands::tts::speak_text,
            commands::tts::stop_tts,
            commands::tts::get_tts_config,
            commands::tts::get_default_tts_voice,
            commands::tts::list_custom_voice_models,
            commands::tts::check_virtual_audio_driver,
            commands::tts::open_url,
            commands::tts::run_tts_system_doctor_playback,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // Avoid panic dialog on end-user machines; log and exit gracefully.
            eprintln!("localtrans failed to start tauri runtime: {}", e);
            tracing::error!("tauri runtime startup failed: {}", e);
        });
}

fn maybe_autostart_cli_worker() {
    if std::env::var("LOCALTRANS_DISABLE_GUI_AUTO_CLI")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let cli = exe.with_file_name("localtrans-cli.exe");
    if !cli.exists() {
        return;
    }
    let workdir = cli.parent().unwrap_or_else(|| std::path::Path::new("."));

    #[cfg(target_os = "windows")]
    {
        let cli_path = cli.clone();
        let workdir_path = workdir.to_path_buf();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(900));
            let ps = format!(
                "Start-Process -FilePath '{}' -ArgumentList 'session-start --no-gui' -WorkingDirectory '{}' -WindowStyle Hidden",
                cli_path.to_string_lossy().replace('\'', "''"),
                workdir_path.to_string_lossy().replace('\'', "''")
            );
            let _ = Command::new("powershell")
                .args(["-NoProfile", "-Command", &ps])
                .spawn();
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        let cli_path = cli.clone();
        let workdir_path = workdir.to_path_buf();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(900));
            let _ = Command::new(cli_path)
                .arg("session-start")
                .arg("--no-gui")
                .current_dir(workdir_path)
                .spawn();
        });
    }
}
