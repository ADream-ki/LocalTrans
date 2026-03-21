//! LocalTrans - Real-time Audio Transcription and Translation
//!
//! This is the main library entry point for the Tauri application.
//! It initializes all components and registers Tauri commands.

mod commands;
mod audio;
pub mod asr;
pub mod translation;
mod pipeline;
pub mod tts;

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
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
            #[cfg(debug_assertions)]
            {
                tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::DEBUG)
                    .with_target(false)
                    .pretty()
                    .init();
                
                // Open DevTools in development mode
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            
            #[cfg(not(debug_assertions))]
            {
                tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::INFO)
                    .with_target(false)
                    .compact()
                    .init();
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
