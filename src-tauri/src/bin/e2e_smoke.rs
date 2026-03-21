use anyhow::Result;
#[cfg(feature = "mock-asr")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(feature = "mock-asr")]
use std::thread;
#[cfg(feature = "mock-asr")]
use std::time::Duration;
#[cfg(feature = "mock-asr")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "mock-asr")]
use localtrans_lib::asr::{AsrConfig, AsrEngine, MockAsrEngine};
#[cfg(feature = "mock-asr")]
use localtrans_lib::translation::{NllbTranslator, Translator};
#[cfg(feature = "mock-asr")]
use localtrans_lib::tts::{EdgeTtsEngine, TtsEngine};

#[cfg(feature = "mock-asr")]
fn main() -> Result<()> {
    println!("=== LocalTrans E2E Smoke Test ===");
    println!("Flow: record -> ASR(mock) -> translate -> TTS(edge)");

    let host = cpal::default_host();
    let input_devices = host.input_devices()?.count();
    println!("Input devices: {}", input_devices);

    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No default input device"))?;
    let supported_config = device.default_input_config()?;
    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels() as usize;

    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let buf_ref = buffer.clone();
    let err_fn = |err| eprintln!("capture error: {err}");

    let stream = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &supported_config.into(),
            move |data: &[f32], _| {
                let mut out = buf_ref.lock().expect("buffer lock poisoned");
                if channels <= 1 {
                    out.extend_from_slice(data);
                } else {
                    for frame in data.chunks(channels) {
                        let sum: f32 = frame.iter().copied().sum();
                        out.push(sum / channels as f32);
                    }
                }
            },
            err_fn,
            None,
        )?,
        other => anyhow::bail!("Unsupported sample format for smoke test: {other:?}"),
    };

    stream.play()?;
    thread::sleep(Duration::from_secs(2));
    drop(stream);

    let samples = buffer.lock().expect("buffer lock poisoned").clone();
    println!("Captured samples: {} @ {} Hz", samples.len(), sample_rate);

    if samples.is_empty() || sample_rate == 0 {
        anyhow::bail!("Audio capture produced no data");
    }

    let mut asr = MockAsrEngine::init(AsrConfig::default())?;
    let transcription = asr.transcribe(&samples, sample_rate)?;
    println!("ASR text: {}", transcription.text);

    let mut translator = NllbTranslator::init(std::path::Path::new("."))?;
    let translated = translator.translate(&transcription.text, "zh", "en")?;
    println!("Translated text: {}", translated.text);

    let tts = EdgeTtsEngine::new()?;
    let rt = tokio::runtime::Runtime::new()?;
    let audio = rt.block_on(async {
        tts.synthesize(&translated.text, "en-US-JennyNeural").await
    })?;
    println!(
        "TTS synthesized: {:.2}s, {} samples",
        audio.duration_secs,
        audio.samples.len()
    );

    println!("E2E smoke test passed.");
    Ok(())
}

#[cfg(not(feature = "mock-asr"))]
fn main() -> Result<()> {
    anyhow::bail!("e2e_smoke requires --features mock-asr")
}
