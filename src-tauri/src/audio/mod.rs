#![allow(dead_code)]
pub mod capture;
pub mod vad;
pub mod buffer;
pub mod resampler;
pub mod processor;

pub use capture::AudioCapture;
pub use vad::{VadDetector, VadResult};
pub use buffer::AudioBuffer;
pub use resampler::resample_linear;

/// Standard sample rate for ASR (Whisper standard)
pub const ASR_SAMPLE_RATE: u32 = 16000;

/// Default chunk duration in milliseconds
pub const DEFAULT_CHUNK_DURATION_MS: u32 = 600;

/// VAD frame duration in milliseconds
pub const VAD_FRAME_DURATION_MS: u32 = 30;

/// Calculate chunk size from sample rate and duration
pub fn chunk_size_from_duration(sample_rate: u32, duration_ms: u32) -> usize {
    (sample_rate as f64 * duration_ms as f64 / 1000.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_size_calculation() {
        let size = chunk_size_from_duration(16000, 600);
        assert_eq!(size, 9600); // 16kHz * 600ms = 9600 samples
    }
}

