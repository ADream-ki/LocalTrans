#![allow(dead_code)]
use std::collections::VecDeque;

pub struct AudioBuffer {
    buffer: VecDeque<f32>,
    capacity: usize,
    chunk_size: usize,
}

impl AudioBuffer {
    pub fn new(capacity: usize, chunk_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            chunk_size,
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        for sample in samples {
            if self.buffer.len() >= self.capacity {
                self.buffer.pop_front();
            }
            self.buffer.push_back(*sample);
        }
    }

    pub fn get_chunk(&mut self) -> Option<Vec<f32>> {
        if self.buffer.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.chunk_size).collect();
            Some(chunk)
        } else {
            None
        }
    }

    pub fn peek_chunk(&self) -> Option<Vec<f32>> {
        if self.buffer.len() >= self.chunk_size {
            Some(self.buffer.iter().take(self.chunk_size).copied().collect())
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_get() {
        let mut buffer = AudioBuffer::new(100, 10);

        buffer.push(&[1.0; 15]);
        assert_eq!(buffer.len(), 15);

        let chunk = buffer.get_chunk();
        assert!(chunk.is_some());
        assert_eq!(chunk.unwrap().len(), 10);
        assert_eq!(buffer.len(), 5);
    }

    #[test]
    fn test_capacity_limit() {
        let mut buffer = AudioBuffer::new(10, 5);

        buffer.push(&[1.0; 20]);
        assert_eq!(buffer.len(), 10);
    }
}

