#![allow(dead_code)]
use anyhow::Result;

/// Audio processor for preprocessing before ASR
pub struct AudioProcessor {
    target_sample_rate: u32,
    normalize: bool,
    trim_silence: bool,
    silence_threshold: f32,
}

impl AudioProcessor {
    pub fn new(target_sample_rate: u32) -> Self {
        Self {
            target_sample_rate,
            normalize: true,
            trim_silence: true,
            silence_threshold: 0.01,
        }
    }

    /// Enable/disable normalization
    pub fn with_normalize(mut self, enable: bool) -> Self {
        self.normalize = enable;
        self
    }

    /// Enable/disable silence trimming
    pub fn with_trim_silence(mut self, enable: bool) -> Self {
        self.trim_silence = enable;
        self
    }

    /// Set silence threshold
    pub fn with_silence_threshold(mut self, threshold: f32) -> Self {
        self.silence_threshold = threshold;
        self
    }

    /// Process audio samples
    pub fn process(&self, samples: &[f32]) -> Result<ProcessedAudio> {
        let mut processed = samples.to_vec();

        // Normalize if enabled
        if self.normalize {
            processed = self.normalize_audio(&processed);
        }

        // Trim silence if enabled
        let (start, end) = if self.trim_silence {
            self.find_speech_boundaries(&processed)
        } else {
            (0, processed.len())
        };

        let trimmed = if start < end {
            processed[start..end].to_vec()
        } else {
            processed
        };

        let processed_length = trimmed.len();
        Ok(ProcessedAudio {
            samples: trimmed,
            sample_rate: self.target_sample_rate,
            original_length: samples.len(),
            processed_length,
        })
    }

    /// Normalize audio to [-1.0, 1.0] range
    fn normalize_audio(&self, samples: &[f32]) -> Vec<f32> {
        let max_amplitude = samples
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, |a, b| a.max(b));

        if max_amplitude > 0.0 {
            samples.iter().map(|s| s / max_amplitude).collect()
        } else {
            samples.to_vec()
        }
    }

    /// Find speech boundaries using energy-based VAD
    fn find_speech_boundaries(&self, samples: &[f32]) -> (usize, usize) {
        let window_size = self.target_sample_rate as usize / 100 ; // 10ms windows
        let mut start = 0;
        let mut end = samples.len();

        // Find first speech frame
        for (i, window) in samples.windows(window_size).enumerate() {
            let energy: f32 = window.iter().map(|s| s * s).sum::<f32>().sqrt() / window_size as f32;
            if energy > self.silence_threshold {
                start = i * window_size;
                break;
            }
        }

        // Find last speech frame
        for (i, window) in samples.windows(window_size).enumerate().rev() {
            let energy: f32 = window.iter().map(|s| s * s).sum::<f32>().sqrt() / window_size as f32;
            if energy > self.silence_threshold {
                end = ((i + 1) * window_size).min(samples.len());
                break;
            }
        }

        (start, end)
    }
}

/// Result of audio processing
#[derive(Debug, Clone)]
pub struct ProcessedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub original_length: usize,
    pub processed_length: usize,
}

impl ProcessedAudio {
    /// Get duration in seconds
    pub fn duration_secs(&self) -> f32 {
        self.processed_length as f32 / self.sample_rate as f32
    }
}

/// Convert stereo to mono
pub fn stereo_to_mono(stereo: &[f32]) -> Vec<f32> {
    stereo
        .chunks(2)
        .map(|chunk| {
            (chunk.first().copied().unwrap_or(0.0) + chunk.get(1).copied().unwrap_or(0.0)) * 0.5
        })
        .collect()
}

/// Convert i16 samples to f32
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|s| *s as f32 / 32768.0).collect()
}

/// Convert f32 samples to i16
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|s| (*s * 32767.0).clamp(-32768.0, 32767.0) as i16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stereo_to_mono() {
        let stereo = vec![1.0, 0.5, 0.5, 1.0];
        let mono = stereo_to_mono(&stereo);
        assert_eq!(mono, vec![0.75, 0.75]);
    }

    #[test]
    fn test_i16_f32_conversion() {
        let i16_samples = vec![0, 16384, -16384, 32767, -32768];
        let f32_samples = i16_to_f32(&i16_samples);
        
        assert!((f32_samples[0] - 0.0).abs() < 0.001);
        assert!((f32_samples[1] - 0.5).abs() < 0.01);
        assert!((f32_samples[2] - (-0.5)).abs() < 0.01);
    }
}

