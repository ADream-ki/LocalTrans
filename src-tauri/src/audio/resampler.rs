#![allow(dead_code)]
use anyhow::Result;
use rubato::{FftFixedIn, Resampler as RubatoResampler};

/// High-quality audio resampler using rubato
pub struct AudioResampler {
    resampler: FftFixedIn<f32>,
    from_rate: u32,
    to_rate: u32,
    channels: usize,
}

impl AudioResampler {
    /// Create a new resampler
    pub fn new(from_rate: u32, to_rate: u32, channels: usize) -> Result<Self> {
        let chunk_size = 1024;
        
        let resampler = FftFixedIn::<f32>::new(
            from_rate as usize,
            to_rate as usize,
            chunk_size,
            2, // sub_chunks
            channels,
        )?;

        Ok(Self {
            resampler,
            from_rate,
            to_rate,
            channels,
        })
    }

    /// Resample audio to target sample rate
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        // Convert mono to multi-channel format expected by rubato
        let input_vecs: Vec<Vec<f32>> = (0..self.channels)
            .map(|ch| {
                input
                    .iter()
                    .skip(ch)
                    .step_by(self.channels)
                    .copied()
                    .collect()
            })
            .collect();

        let output = self.resampler.process(&input_vecs, None)?;

        // Interleave output channels
        let output_len = output.get(0).map(|v| v.len()).unwrap_or(0);
        let mut result = vec![0.0f32; output_len * self.channels];
        
        for (ch, channel_data) in output.iter().enumerate() {
            for (i, sample) in channel_data.iter().enumerate() {
                result[i * self.channels + ch] = *sample;
            }
        }

        Ok(result)
    }

    /// Get resampling ratio
    pub fn ratio(&self) -> f64 {
        self.to_rate as f64 / self.from_rate as f64
    }

    /// Get input sample rate
    pub fn from_rate(&self) -> u32 {
        self.from_rate
    }

    /// Get output sample rate
    pub fn to_rate(&self) -> u32 {
        self.to_rate
    }
}

/// Simple linear interpolation resampler (faster but lower quality)
pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        
        if src_idx + 1 < input.len() {
            let frac = src_pos - src_idx as f64;
            let sample = input[src_idx] * (1.0 - frac as f32) + input[src_idx + 1] * frac as f32;
            output.push(sample);
        } else if src_idx < input.len() {
            output.push(input[src_idx]);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_resample() {
        let input = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let output = resample_linear(&input, 16000, 8000);
        
        // Should be approximately half the length
        assert!(output.len() >= 2 && output.len() <= 3);
    }

    #[test]
    fn test_same_rate() {
        let input = vec![0.0, 0.5, 1.0];
        let output = resample_linear(&input, 16000, 16000);
        assert_eq!(output, input);
    }
}
