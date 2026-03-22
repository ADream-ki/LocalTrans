#![allow(dead_code, clippy::arc_with_non_send_sync)]
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, HostId, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_input: bool,
    pub is_default: bool,
}

/// Thread-safe audio capture wrapper
pub struct AudioCapture {
    host: Host,
    device_id: Option<String>,
    input_sample_rate: u32,
    input_channels: u16,
    is_capturing: Arc<AtomicBool>,
    // Captured samples in mono f32 format at input_sample_rate
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Arc<Mutex<Option<Stream>>>,
}

// Explicitly implement Send since Host is Send on most platforms
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    pub fn new(device_name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        tracing::info!(device_name = ?device_name, "creating audio capture");

        Ok(Self {
            host,
            device_id: device_name.map(|s| s.to_string()),
            input_sample_rate: 0,
            input_channels: 0,
            is_capturing: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: Arc::new(Mutex::new(None)),
        })
    }

    pub fn list_devices() -> Result<Vec<AudioDevice>> {
        let default_host = cpal::default_host();
        let default_host_id = default_host.id();
        let default_input = default_host.default_input_device().and_then(|d| d.name().ok());
        let default_output = default_host.default_output_device().and_then(|d| d.name().ok());
        let mut all: Vec<AudioDevice> = Vec::new();
        let mut seen = HashSet::<String>::new();

        for host_id in cpal::available_hosts() {
            let Ok(host) = cpal::host_from_id(host_id) else {
                continue;
            };
            for (idx, d) in host.input_devices()?.enumerate() {
                let Ok(name) = d.name() else { continue };
                let id = format!("host::{:?}::input::{}::{}", host_id, idx, name);
                if seen.insert(id.clone()) {
                    let is_default = host_id == default_host_id
                        && default_input.as_ref().map(|n| n == &name).unwrap_or(false);
                    all.push(AudioDevice {
                        id,
                        name,
                        is_input: true,
                        is_default,
                    });
                }
            }
            for (idx, d) in host.output_devices()?.enumerate() {
                let Ok(name) = d.name() else { continue };
                let id = format!("host::{:?}::output::{}::{}", host_id, idx, name);
                if seen.insert(id.clone()) {
                    let is_default = host_id == default_host_id
                        && default_output.as_ref().map(|n| n == &name).unwrap_or(false);
                    all.push(AudioDevice {
                        id,
                        name,
                        is_input: false,
                        is_default,
                    });
                }
            }
        }

        tracing::info!(
            total = all.len(),
            default_input = ?default_input,
            default_output = ?default_output,
            "audio devices listed"
        );
        Ok(all)
    }

    pub fn start_capture(&mut self) -> Result<()> {
        tracing::info!(requested_device = ?self.device_id, "starting audio capture");
        let device = if let Some(ref requested) = self.device_id {
            select_input_device(requested)?
        } else {
            self.host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No default input device"))?
        };

        let device_name = device.name().unwrap_or_else(|_| "<unknown-input-device>".to_string());
        let supported_config = device.default_input_config()?;
        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.into();

        self.input_sample_rate = config.sample_rate.0;
        self.input_channels = config.channels;
        tracing::info!(
            device = %device_name,
            sample_rate = self.input_sample_rate,
            channels = self.input_channels,
            sample_format = ?sample_format,
            "audio capture config selected"
        );

        self.is_capturing.store(true, Ordering::SeqCst);

        let buffer = self.buffer.clone();
        let is_capturing = self.is_capturing.clone();
        let channels = config.channels as usize;

        let err_fn = |err| tracing::error!("Audio capture error: {}", err);

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !is_capturing.load(Ordering::SeqCst) {
                        return;
                    }
                    let mut buf = buffer.lock();
                    if channels <= 1 {
                        buf.extend(data.iter().copied());
                        return;
                    }
                    for frame in data.chunks(channels) {
                        let mut sum = 0.0f32;
                        for &s in frame {
                            sum += s;
                        }
                        buf.push(sum / channels as f32);
                    }
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !is_capturing.load(Ordering::SeqCst) {
                        return;
                    }
                    let mut buf = buffer.lock();
                    if channels <= 1 {
                        buf.extend(data.iter().map(|s| *s as f32 / 32768.0));
                        return;
                    }
                    for frame in data.chunks(channels) {
                        let mut sum = 0.0f32;
                        for &s in frame {
                            sum += s as f32 / 32768.0;
                        }
                        buf.push(sum / channels as f32);
                    }
                },
                err_fn,
                None,
            )?,
            SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    if !is_capturing.load(Ordering::SeqCst) {
                        return;
                    }
                    let mut buf = buffer.lock();
                    if channels <= 1 {
                        buf.extend(data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0));
                        return;
                    }
                    for frame in data.chunks(channels) {
                        let mut sum = 0.0f32;
                        for &s in frame {
                            sum += (s as f32 - 32768.0) / 32768.0;
                        }
                        buf.push(sum / channels as f32);
                    }
                },
                err_fn,
                None,
            )?,
            other => {
                return Err(anyhow::anyhow!(
                    "Unsupported input sample format: {:?}",
                    other
                ));
            }
        };

        stream.play()?;
        *self.stream.lock() = Some(stream);
        tracing::info!("audio capture started");

        Ok(())
    }

    pub fn stop_capture(&mut self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        *self.stream.lock() = None;
        tracing::info!("audio capture stopped");
    }

    pub fn get_samples(&self) -> Vec<f32> {
        let mut buf = self.buffer.lock();
        std::mem::take(&mut *buf)
    }

    pub fn sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    pub fn channels(&self) -> u16 {
        // Buffer is mono after downmixing
        1
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop_capture();
    }
}

fn select_input_device(requested: &str) -> Result<Device> {
    let mut exact_new: Option<Device> = None;
    let mut legacy_or_name: Option<Device> = None;
    let mut host_specific: Option<(HostId, usize)> = None;

    if let Some(rest) = requested.strip_prefix("host::") {
        let mut parts = rest.splitn(4, "::");
        let host_tag = parts.next();
        let io_tag = parts.next();
        let idx_str = parts.next();
        if host_tag.is_some() && io_tag == Some("input") {
            if let Some(idx_raw) = idx_str {
                if let Ok(idx) = idx_raw.parse::<usize>() {
                    host_specific = cpal::available_hosts()
                        .into_iter()
                        .find(|hid| format!("{:?}", hid) == host_tag.unwrap_or_default())
                        .map(|hid| (hid, idx));
                }
            }
        }
    }

    if let Some((hid, idx)) = host_specific {
        if let Ok(host) = cpal::host_from_id(hid) {
            for (i, d) in host.input_devices()?.enumerate() {
                if i == idx {
                    return Ok(d);
                }
            }
        }
    }

    for host_id in cpal::available_hosts() {
        let Ok(host) = cpal::host_from_id(host_id) else {
            continue;
        };
        for (idx, d) in host.input_devices()?.enumerate() {
            let Ok(name) = d.name() else { continue };
            let new_id = format!("host::{:?}::input::{}::{}", host_id, idx, name);
            let legacy_id = format!("input::{}::{}", idx, name);
            if requested == new_id {
                exact_new = Some(d);
                break;
            }
            if requested == legacy_id || requested == name {
                legacy_or_name = Some(d);
            }
        }
        if exact_new.is_some() {
            break;
        }
    }

    exact_new
        .or(legacy_or_name)
        .ok_or_else(|| anyhow::anyhow!("Device not found: {}", requested))
}

