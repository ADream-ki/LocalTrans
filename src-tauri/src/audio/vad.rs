#![allow(dead_code)]
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadResult {
    Silence,
    SpeechStart,
    Speech,
    SpeechEnd,
}

pub struct VadDetector {
    threshold: f32,
    frame_size: usize,
    energy_history: VecDeque<f32>,
    speech_frames: usize,
    silence_frames: usize,
    min_speech_frames: usize,
    min_silence_frames: usize,
    is_speech: bool,
}

impl VadDetector {
    pub fn new(sample_rate: u32, frame_duration_ms: u32) -> Self {
        let frame_size = (sample_rate as f32 * frame_duration_ms as f32 / 1000.0) as usize;

        Self {
            threshold: 0.01,
            frame_size,
            energy_history: VecDeque::with_capacity(10),
            speech_frames: 0,
            silence_frames: 0,
            min_speech_frames: 3,
            min_silence_frames: 15,
            is_speech: false,
        }
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    pub fn process(&mut self, frame: &[f32]) -> VadResult {
        let energy = Self::compute_energy(frame);

        self.energy_history.push_back(energy);
        if self.energy_history.len() > 10 {
            self.energy_history.pop_front();
        }

        let adaptive_threshold = self.compute_adaptive_threshold();
        let is_speech_frame = energy > adaptive_threshold;

        if self.is_speech {
            if is_speech_frame {
                self.silence_frames = 0;
                VadResult::Speech
            } else {
                self.silence_frames += 1;
                if self.silence_frames >= self.min_silence_frames {
                    self.is_speech = false;
                    self.speech_frames = 0;
                    VadResult::SpeechEnd
                } else {
                    VadResult::Speech
                }
            }
        } else if is_speech_frame {
            self.speech_frames += 1;
            if self.speech_frames >= self.min_speech_frames {
                self.is_speech = true;
                self.silence_frames = 0;
                VadResult::SpeechStart
            } else {
                VadResult::Silence
            }
        } else {
            self.speech_frames = 0;
            VadResult::Silence
        }
    }

    fn compute_energy(frame: &[f32]) -> f32 {
        let sum: f32 = frame.iter().map(|s| s * s).sum();
        (sum / frame.len() as f32).sqrt()
    }

    fn compute_adaptive_threshold(&self) -> f32 {
        if self.energy_history.is_empty() {
            return self.threshold;
        }

        let avg: f32 = self.energy_history.iter().sum::<f32>() / self.energy_history.len() as f32;
        (avg + self.threshold) * 0.5
    }

    pub fn is_speech(&self) -> bool {
        self.is_speech
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_silence() {
        let mut vad = VadDetector::new(16000, 30);
        let silence = vec![0.0f32; 480];

        for _ in 0..10 {
            let result = vad.process(&silence);
            assert!(matches!(result, VadResult::Silence));
        }
    }

    #[test]
    fn test_vad_speech_detection() {
        let mut vad = VadDetector::new(16000, 30);
        let speech = vec![0.5f32; 480];

        // Process speech frames
        for _ in 0..5 {
            vad.process(&speech);
        }

        assert!(vad.is_speech());
    }
}

