#![allow(dead_code)]
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

pub enum StreamEvent {
    AudioChunk(Vec<f32>),
    Transcription(String),
    Translation(String),
    Error(String),
}

pub struct TranscriptionStream {
    receiver: mpsc::Receiver<StreamEvent>,
}

impl TranscriptionStream {
    pub fn new(buffer_size: usize) -> (Self, mpsc::Sender<StreamEvent>) {
        let (sender, receiver) = mpsc::channel(buffer_size);
        (Self { receiver }, sender)
    }
}

impl Stream for TranscriptionStream {
    type Item = StreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

