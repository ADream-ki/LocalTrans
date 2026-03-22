//! Audio playback module for TTS output

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use parking_lot::Mutex;

use super::{TtsAudio, TtsEngine};

/// Audio player for TTS output
pub struct AudioPlayer {
    host: Host,
    device_name: Option<String>,
    volume: f32,
}

/// Playback control shared between commands.
///
/// - `begin_new()` cancels any previous playback and returns a new token.
/// - `stop()` cancels the current token.
#[derive(Default)]
pub struct PlaybackControl {
    current_cancel: Arc<AtomicBool>,
}

impl PlaybackControl {
    pub fn begin_new(&mut self) -> Arc<AtomicBool> {
        self.current_cancel.store(true, Ordering::SeqCst);
        let token = Arc::new(AtomicBool::new(false));
        self.current_cancel = token.clone();
        token
    }

    pub fn stop(&self) {
        self.current_cancel.store(true, Ordering::SeqCst);
    }
}

impl AudioPlayer {
    /// Create a new audio player
    pub fn new(device_name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();

        Ok(Self {
            host,
            device_name: device_name.map(|s| s.to_string()),
            volume: 1.0,
        })
    }

    /// List available output devices
    pub fn list_devices() -> Result<Vec<OutputDevice>> {
        let default_host = cpal::default_host();
        let default_host_id = default_host.id();
        let default_output = default_host.default_output_device()
            .and_then(|d| d.name().ok());
        let mut devices: Vec<OutputDevice> = Vec::new();
        let mut seen = HashSet::<String>::new();

        for host_id in cpal::available_hosts() {
            let Ok(host) = cpal::host_from_id(host_id) else {
                continue;
            };
            for (idx, d) in host.output_devices()?.enumerate() {
                let Ok(name) = d.name() else { continue };
                let id = format!("host::{:?}::output::{}::{}", host_id, idx, name);
                if seen.insert(id.clone()) {
                    let is_default = host_id == default_host_id
                        && default_output.as_ref().map(|n| n == &name).unwrap_or(false);
                    devices.push(OutputDevice {
                        id,
                        name,
                        is_default,
                    });
                }
            }
        }

        Ok(devices)
    }

    /// Set output device
    pub fn set_device(&mut self, device_name: &str) {
        self.device_name = Some(device_name.to_string());
    }

    /// Set volume (0.0 - 1.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    /// Play audio (blocking)
    pub fn play(&self, audio: &TtsAudio) -> Result<()> {
        let device = self.get_device()?;
        let config = device.default_output_config()?;

        let output_sample_rate = config.sample_rate().0;
        let output_channels = config.channels() as usize;

        // Resample if needed
        let samples = if audio.sample_rate != output_sample_rate {
            resample_audio(&audio.samples, audio.sample_rate, output_sample_rate)
        } else {
            audio.samples.clone()
        };

        // Convert mono to stereo if needed
        let samples = if audio.channels == 1 && output_channels == 2 {
            mono_to_stereo(&samples)
        } else {
            samples
        };

        // Calculate duration before moving samples
        let duration_ms = (samples.len() as f32 / output_sample_rate as f32 * 1000.0) as u64;

        let samples = Arc::new(Mutex::new(samples));
        let sample_index = Arc::new(Mutex::new(0usize));
        let volume = self.volume;

        let stream = device.build_output_stream(
            &config.into(),
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut idx = sample_index.lock();
                let samples = samples.lock();
                
                for sample in output.iter_mut() {
                    if *idx < samples.len() {
                        *sample = samples[*idx] * volume;
                        *idx += 1;
                    } else {
                        *sample = 0.0;
                    }
                }
            },
            |err| tracing::error!("Audio playback error: {}", err),
            None,
        )?;

        stream.play()?;

        // Wait for playback to complete
        std::thread::sleep(std::time::Duration::from_millis(duration_ms + 100));

        Ok(())
    }

    /// Play audio asynchronously
    pub async fn play_async(&self, audio: TtsAudio) -> Result<()> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.play_async_cancellable(audio, cancel).await
    }

    /// Play audio asynchronously with cancellation
    pub async fn play_async_cancellable(
        &self,
        audio: TtsAudio,
        cancel: Arc<AtomicBool>,
    ) -> Result<()> {
        let samples = audio.samples.clone();
        let sample_rate = audio.sample_rate;
        let channels = audio.channels;
        let volume = self.volume;
        let device_name = self.device_name.clone();
        
        tokio::task::spawn_blocking(move || {
            play_audio_blocking(
                &samples,
                sample_rate,
                channels,
                volume,
                device_name.as_deref(),
                cancel,
            )
        }).await?
    }

    fn get_device(&self) -> Result<Device> {
        if let Some(ref wanted) = self.device_name {
            find_output_device_across_hosts(wanted)
        } else {
            self.host.default_output_device()
                .ok_or_else(|| anyhow::anyhow!("No default output device"))
        }
    }
}

/// Output device info
#[derive(Debug, Clone)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Resample audio using linear interpolation
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        
        if src_idx + 1 < samples.len() {
            let frac = src_pos - src_idx as f64;
            let sample = samples[src_idx] * (1.0 - frac as f32) + samples[src_idx + 1] * frac as f32;
            output.push(sample);
        } else if src_idx < samples.len() {
            output.push(samples[src_idx]);
        }
    }

    output
}

/// Convert mono audio to stereo
fn mono_to_stereo(mono: &[f32]) -> Vec<f32> {
    mono.iter()
        .flat_map(|&sample| [sample, sample])
        .collect()
}

/// Blocking audio playback function (for use in spawn_blocking)
fn play_audio_blocking(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    volume: f32,
    device_name: Option<&str>,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let host = cpal::default_host();
    
    let device = if let Some(name) = device_name {
        find_output_device_across_hosts(name)?
    } else {
        host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default output device"))?
    };

    let config = device.default_output_config()?;
    let output_sample_rate = config.sample_rate().0;
    let output_channels = config.channels() as usize;

    // Resample if needed
    let samples = if sample_rate != output_sample_rate {
        resample_audio(samples, sample_rate, output_sample_rate)
    } else {
        samples.to_vec()
    };

    // Convert mono to stereo if needed
    let samples = if channels == 1 && output_channels == 2 {
        mono_to_stereo(&samples)
    } else {
        samples
    };

    // Calculate duration before moving samples
    let duration_ms = (samples.len() as f32 / output_sample_rate as f32 * 1000.0) as u64;

    let samples = Arc::new(Mutex::new(samples));
    let sample_index = Arc::new(Mutex::new(0usize));
    let cancel_flag = cancel.clone();

    let stream = device.build_output_stream(
        &config.into(),
        move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if cancel_flag.load(Ordering::SeqCst) {
                for sample in output.iter_mut() {
                    *sample = 0.0;
                }
                return;
            }
            let mut idx = sample_index.lock();
            let samples = samples.lock();
            
            for sample in output.iter_mut() {
                if *idx < samples.len() {
                    *sample = samples[*idx] * volume;
                    *idx += 1;
                } else {
                    *sample = 0.0;
                }
            }
        },
        |err| tracing::error!("Audio playback error: {}", err),
        None,
    )?;

    stream.play()?;

    // Wait for playback to complete or be cancelled
    let start = Instant::now();
    let total = Duration::from_millis(duration_ms + 100);
    while start.elapsed() < total {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    Ok(())
}

fn find_output_device_across_hosts(wanted: &str) -> Result<Device> {
    if let Some(rest) = wanted.strip_prefix("host::") {
        let mut parts = rest.splitn(4, "::");
        let host_tag = parts.next();
        let io_tag = parts.next();
        let idx_str = parts.next();
        if host_tag.is_some() && io_tag == Some("output") {
            if let Some(idx_raw) = idx_str {
                if let Ok(idx) = idx_raw.parse::<usize>() {
                    if let Some(hid) = cpal::available_hosts()
                        .into_iter()
                        .find(|hid| format!("{:?}", hid) == host_tag.unwrap_or_default())
                    {
                        if let Ok(host) = cpal::host_from_id(hid) {
                            for (i, d) in host.output_devices()?.enumerate() {
                                if i == idx {
                                    return Ok(d);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut by_name: Option<Device> = None;
    for host_id in cpal::available_hosts() {
        let Ok(host) = cpal::host_from_id(host_id) else {
            continue;
        };
        for (idx, d) in host.output_devices()?.enumerate() {
            let Ok(name) = d.name() else { continue };
            let full_id = format!("host::{:?}::output::{}::{}", host_id, idx, name);
            let legacy_id = name.clone();
            if wanted == full_id {
                return Ok(d);
            }
            if wanted == legacy_id || wanted == name {
                by_name = Some(d);
            }
        }
    }
    by_name.ok_or_else(|| anyhow::anyhow!("Output device not found: {}", wanted))
}

/// TTS Playback service for real-time translation output
pub struct TtsPlaybackService {
    player: AudioPlayer,
    enabled: bool,
    voice: String,
}

impl TtsPlaybackService {
    pub fn new(voice: &str) -> Result<Self> {
        Ok(Self {
            player: AudioPlayer::new(None)?,
            enabled: true,
            voice: voice.to_string(),
        })
    }

    /// Enable or disable TTS playback
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set voice
    pub fn set_voice(&mut self, voice: &str) {
        self.voice = voice.to_string();
    }

    /// Set volume
    pub fn set_volume(&mut self, volume: f32) {
        self.player.set_volume(volume);
    }

    /// Speak text using TTS
    pub async fn speak(&self, text: &str) -> Result<()> {
        if !self.enabled || text.trim().is_empty() {
            return Ok(());
        }

        // Use edge-tts for synthesis
        let engine = crate::tts::edge_tts::EdgeTtsEngine::new()?;
        let audio = engine.synthesize(text, &self.voice).await?;
        
        // Play the audio
        self.player.play_async(audio).await
    }

    /// Stop current playback
    pub fn stop(&self) {
        // Audio playback will naturally end
    }
}
