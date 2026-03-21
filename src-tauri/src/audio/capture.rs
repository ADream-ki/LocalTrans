#![allow(dead_code)]
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Host, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
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
    device_name: Option<String>,
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

        Ok(Self {
            host,
            device_name: device_name.map(|s| s.to_string()),
            input_sample_rate: 0,
            input_channels: 0,
            is_capturing: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: Arc::new(Mutex::new(None)),
        })
    }

    pub fn list_devices() -> Result<Vec<AudioDevice>> {
        let host = cpal::default_host();
        let default_input = host.default_input_device().and_then(|d| d.name().ok());

        let input_devices: Vec<AudioDevice> = host
            .input_devices()?
            .filter_map(|d| {
                let name = d.name().ok()?;
                let is_default = default_input.as_ref().map(|n| n == &name).unwrap_or(false);
                Some(AudioDevice {
                    id: name.clone(),
                    name,
                    is_input: true,
                    is_default,
                })
            })
            .collect();

        let default_output = host.default_output_device().and_then(|d| d.name().ok());

        let output_devices: Vec<AudioDevice> = host
            .output_devices()?
            .filter_map(|d| {
                let name = d.name().ok()?;
                let is_default = default_output.as_ref().map(|n| n == &name).unwrap_or(false);
                Some(AudioDevice {
                    id: name.clone(),
                    name,
                    is_input: false,
                    is_default,
                })
            })
            .collect();

        Ok(input_devices.into_iter().chain(output_devices).collect())
    }

    pub fn start_capture(&mut self) -> Result<()> {
        let device = if let Some(ref name) = self.device_name {
            self.host
                .input_devices()?
                .find(|d| d.name().map(|n| &n == name).unwrap_or(false))
                .ok_or_else(|| anyhow::anyhow!("Device not found: {}", name))?
        } else {
            self.host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No default input device"))?
        };

        let supported_config = device.default_input_config()?;
        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.into();

        self.input_sample_rate = config.sample_rate.0;
        self.input_channels = config.channels;

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

        Ok(())
    }

    pub fn stop_capture(&mut self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        *self.stream.lock() = None;
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

