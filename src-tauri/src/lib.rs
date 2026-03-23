pub mod commands;
pub mod error;
pub mod ipc;
pub mod audio;
pub mod asr;
pub mod translation;
pub mod tts;
pub mod pipeline;
pub mod session_bus;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .setup(|app| {
            ipc::start_gui_ipc_server(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::hello::hello,
            commands::version::version,
            commands::process_file::process_file,
            commands::session::start_session,
            commands::session::stop_session,
            commands::session::pause_session,
            commands::session::resume_session,
            commands::session::get_session_stats,
            commands::session::get_session_status,
            commands::session::get_session_history,
            commands::session::clear_session_history,
            commands::session::export_history,
            commands::session::update_languages,
            commands::audio::get_audio_devices,
            commands::audio::get_tts_output_devices,
            commands::audio::check_virtual_audio_driver,
            commands::audio::start_capture,
            commands::audio::stop_capture,
            commands::model::get_models_dir,
            commands::model::list_models,
            commands::model::download_model,
            commands::model::delete_model,
            commands::config::set_app_config,
            commands::config::get_app_config,
            commands::config::set_config_value,
            commands::config::get_config_value,
            commands::system::get_runtime_status,
            commands::system::get_log_status,
            commands::system::open_url,
            commands::tts::get_tts_voices,
            commands::tts::speak_text,
            commands::tts::stop_tts,
            commands::tts::get_tts_config,
            commands::tts::get_default_tts_voice,
            commands::tts::list_custom_voice_models,
            commands::tts::run_tts_system_doctor_playback,
            commands::translation::translate_text
        ])
        .run(tauri::generate_context!())
}
