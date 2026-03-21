#![allow(dead_code)]
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;


#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionStatus {
    Idle,
    Running,
    Paused,
    Error,
}

/// Thread-safe transcription session
pub struct TranscriptionSession {
    status: Arc<Mutex<SessionStatus>>,
    source_lang: String,
    target_lang: String,
    input_device: Option<String>,
    current_source: Arc<Mutex<String>>,
    current_translation: Arc<Mutex<String>>,
    is_running: Arc<AtomicBool>,
    history: Arc<Mutex<Vec<TranscriptionHistoryItem>>>,
}

// Explicitly implement Send and Sync
unsafe impl Send for TranscriptionSession {}
unsafe impl Sync for TranscriptionSession {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionHistoryItem {
    pub id: String,
    pub source_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub timestamp: String,
    pub loci_enhanced: bool,
}

impl TranscriptionSession {
    pub fn new(
        source_lang: String,
        target_lang: String,
        input_device: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            status: Arc::new(Mutex::new(SessionStatus::Idle)),
            source_lang,
            target_lang,
            input_device,
            current_source: Arc::new(Mutex::new(String::new())),
            current_translation: Arc::new(Mutex::new(String::new())),
            is_running: Arc::new(AtomicBool::new(false)),
            history: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        *self.status.lock() = SessionStatus::Running;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.is_running.store(false, Ordering::SeqCst);
        *self.status.lock() = SessionStatus::Idle;
        *self.current_source.lock() = String::new();
        *self.current_translation.lock() = String::new();
        Ok(())
    }

    pub fn pause(&mut self) {
        *self.status.lock() = SessionStatus::Paused;
    }

    pub fn resume(&mut self) {
        *self.status.lock() = SessionStatus::Running;
    }

    pub fn get_status(&self) -> SessionStatus {
        *self.status.lock()
    }

    pub fn get_current_source(&self) -> String {
        self.current_source.lock().clone()
    }

    pub fn get_current_translation(&self) -> String {
        self.current_translation.lock().clone()
    }

    pub fn get_history(&self) -> Vec<TranscriptionHistoryItem> {
        self.history.lock().clone()
    }
}

impl Drop for TranscriptionSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
