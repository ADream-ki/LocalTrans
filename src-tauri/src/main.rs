mod cli;
mod env_path;

use std::io::{self, IsTerminal};
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::Parser;
use colored::Colorize;
use localtrans_lib::commands;
use localtrans_lib::error::AppError;
use localtrans_lib::ipc::{try_send_command, IpcCommand};
use serde::Serialize;
use serde_json::json;
use tracing_subscriber::EnvFilter;

use crate::cli::parser::{Cli, Commands};

fn main() -> ExitCode {
    #[cfg(target_os = "windows")]
    hide_console_for_gui_mode();

    if std::env::var_os("NO_COLOR").is_some() {
        colored::control::set_override(false);
    }

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let _ = err.print();
                    return ExitCode::SUCCESS;
                }
                _ => {}
            }

            if !io::stderr().is_terminal() && std::env::args_os().len() > 1 {
                let _ = localtrans_lib::run();
                return ExitCode::SUCCESS;
            }
            let _ = err.print();
            return ExitCode::from(2);
        }
    };

    init_tracing(cli.verbose);

    match cli.command {
        Some(command) => {
            match try_send_command(to_ipc_command(&command)) {
                Ok(Some(payload)) => {
                    if let Err(err) = emit_json_value(payload) {
                        emit_error(err);
                        return ExitCode::from(1);
                    }
                    return ExitCode::SUCCESS;
                }
                Ok(None) => {}
                Err(err) => {
                    emit_error(AppError::InvalidState(format!("ipc dispatch failed: {err}")));
                    return ExitCode::from(1);
                }
            }

            match run_cli(command) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    emit_error(err);
                    ExitCode::from(1)
                }
            }
        }
        None => {
            let _path_guard = env_path::register_for_gui_mode();
            match localtrans_lib::run() {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    emit_error(AppError::Io(format!("GUI runtime error: {err}")));
                    ExitCode::from(1)
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn hide_console_for_gui_mode() {
    // For no-arg GUI launch, hide the auto-created console window.
    // For CLI launch (has args), keep console visible for output.
    if std::env::args_os().len() > 1 {
        return;
    }

    use windows_sys::Win32::System::Console::{GetConsoleProcessList, GetConsoleWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    let mut processes = [0u32; 2];
    unsafe {
        let process_count = GetConsoleProcessList(processes.as_mut_ptr(), processes.len() as u32);
        if process_count <= 1 {
            let hwnd = GetConsoleWindow();
            if !hwnd.is_null() {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn run_cli(command: Commands) -> Result<(), AppError> {
    match command {
        Commands::Hello { name } => emit_json(commands::hello::hello(name)),
        Commands::Version => emit_json(commands::version::version()),
        Commands::ProcessFile { input } => emit_json(commands::process_file::process_file(input)),
        Commands::DownloadModel {
            model_id,
            model_type,
        } => emit_json(commands::model::download_model_cli(model_id, model_type)),
        Commands::ListModels { model_type } => emit_json(commands::model::list_models(model_type)),
        Commands::DeleteModel { model_id } => emit_json(commands::model::delete_model(model_id)),
        Commands::SessionStart {
            source_lang,
            target_lang,
            bidirectional,
        } => emit_json(commands::session::start_session_cli(
            source_lang,
            target_lang,
            bidirectional,
        )),
        Commands::SessionPause => emit_json(commands::session::pause_session_cli()),
        Commands::SessionResume => emit_json(commands::session::resume_session_cli()),
        Commands::SessionStop => emit_json(commands::session::stop_session_cli()),
        Commands::SessionStatus => emit_json(commands::session::session_status_cli()),
        Commands::SessionStats => emit_json(commands::session::get_session_stats()),
        Commands::SessionHistory { count } => {
            emit_json(commands::session::get_session_history_cli(Some(count)))
        }
        Commands::SessionClearHistory => emit_json(commands::session::clear_session_history_cli()),
        Commands::SessionExportHistory { output } => emit_json(commands::session::export_history_cli(
            output.map(|p| p.display().to_string()),
        )),
        Commands::SessionUpdateLanguages {
            source_lang,
            target_lang,
        } => emit_json(commands::session::update_languages_cli(source_lang, target_lang)),
        Commands::TranslateText {
            text,
            source_lang,
            target_lang,
        } => emit_json(commands::translation::translate_text_cli(
            text,
            source_lang,
            target_lang,
        )),
        Commands::LogStatus => emit_json(commands::system::get_log_status()),
        Commands::TtsVoices { language } => emit_json(commands::tts::get_tts_voices(language)),
        Commands::TtsConfig => emit_json(commands::tts::get_tts_config()),
        Commands::TtsDefaultVoice { language } => {
            emit_json(commands::tts::get_default_tts_voice(language))
        }
        Commands::TtsCustomVoices { models_dir } => emit_json(commands::tts::list_custom_voice_models(
            models_dir.map(|p| p.display().to_string()),
        )),
        Commands::ConfigSet { key, value } => {
            let parsed = serde_json::from_str::<serde_json::Value>(&value)
                .unwrap_or_else(|_| serde_json::Value::String(value));
            emit_json(commands::config::set_config_value(key, parsed))
        }
        Commands::ConfigGet { key } => emit_json(commands::config::get_config_value(key)),
        Commands::Call { name, args_json } => {
            let args = serde_json::from_str::<serde_json::Value>(&args_json)
                .map_err(|e| AppError::InvalidState(format!("invalid args_json: {e}")))?;
            emit_json_value(commands::router::execute_named(&name, args, None)?)
        }
    }
}

fn to_ipc_command(command: &Commands) -> IpcCommand {
    match command {
        Commands::Hello { name } => IpcCommand::Hello { name: name.clone() },
        Commands::Version => IpcCommand::Version,
        Commands::ProcessFile { input } => IpcCommand::ProcessFile {
            input: input.display().to_string(),
        },
        Commands::DownloadModel {
            model_id,
            model_type,
        } => IpcCommand::DownloadModel {
            model_id: model_id.clone(),
            model_type: model_type.clone(),
        },
        Commands::ListModels { model_type } => IpcCommand::ListModels {
            model_type: model_type.clone(),
        },
        Commands::DeleteModel { model_id } => IpcCommand::DeleteModel {
            model_id: model_id.clone(),
        },
        Commands::SessionStart {
            source_lang,
            target_lang,
            bidirectional,
        } => IpcCommand::SessionStart {
            source_lang: source_lang.clone(),
            target_lang: target_lang.clone(),
            bidirectional: *bidirectional,
        },
        Commands::SessionPause => IpcCommand::SessionPause,
        Commands::SessionResume => IpcCommand::SessionResume,
        Commands::SessionStop => IpcCommand::SessionStop,
        Commands::SessionStatus => IpcCommand::SessionStatus,
        Commands::SessionStats => IpcCommand::SessionStats,
        Commands::SessionHistory { count } => IpcCommand::SessionHistory { count: *count },
        Commands::SessionClearHistory => IpcCommand::SessionClearHistory,
        Commands::SessionExportHistory { output } => IpcCommand::SessionExportHistory {
            output: output.as_ref().map(|p| p.display().to_string()),
        },
        Commands::SessionUpdateLanguages {
            source_lang,
            target_lang,
        } => IpcCommand::SessionUpdateLanguages {
            source_lang: source_lang.clone(),
            target_lang: target_lang.clone(),
        },
        Commands::TranslateText {
            text,
            source_lang,
            target_lang,
        } => IpcCommand::TranslateText {
            text: text.clone(),
            source_lang: source_lang.clone(),
            target_lang: target_lang.clone(),
        },
        Commands::LogStatus => IpcCommand::LogStatus,
        Commands::TtsVoices { language } => IpcCommand::TtsVoices {
            language: language.clone(),
        },
        Commands::TtsConfig => IpcCommand::TtsConfig,
        Commands::TtsDefaultVoice { language } => IpcCommand::TtsDefaultVoice {
            language: language.clone(),
        },
        Commands::TtsCustomVoices { models_dir } => IpcCommand::TtsCustomVoices {
            models_dir: models_dir.as_ref().map(|p| p.display().to_string()),
        },
        Commands::ConfigSet { key, value } => IpcCommand::ConfigSet {
            key: key.clone(),
            value: value.clone(),
        },
        Commands::ConfigGet { key } => IpcCommand::ConfigGet { key: key.clone() },
        Commands::Call { name, args_json } => {
            let args = serde_json::from_str::<serde_json::Value>(args_json)
                .unwrap_or_else(|_| serde_json::json!({}));
            IpcCommand::Call {
                name: name.clone(),
                args,
            }
        }
    }
}

fn emit_json<T: Serialize>(result: Result<T, AppError>) -> Result<(), AppError> {
    let value = result?;
    let body = serde_json::to_string_pretty(&value).map_err(|e| AppError::Io(e.to_string()))?;
    println!("{body}");
    Ok(())
}

fn emit_json_value(value: serde_json::Value) -> Result<(), AppError> {
    let body = serde_json::to_string_pretty(&value).map_err(|e| AppError::Io(e.to_string()))?;
    println!("{body}");
    Ok(())
}

fn emit_error(error: AppError) {
    let payload = json!({ "error": error });
    let pretty = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    eprintln!("{}", "ERROR".red().bold());
    eprintln!("{pretty}");
}
