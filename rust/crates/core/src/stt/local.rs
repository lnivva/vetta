use super::{Stt, SttError, TranscribeOptions, TranscriptChunk, TranscriptStream, Word};
use async_trait::async_trait;
use std::path::Path;
use tokio_stream::StreamExt;
use tonic::Status;

pub mod proto {
    tonic::include_proto!("speech");
}

use crate::common::UdsChannel;
use proto::{
    TranscribeOptions as ProtoOptions, TranscribeRequest,
    speech_to_text_client::SpeechToTextClient, transcribe_request::AudioSource,
};

pub struct LocalSttStrategy {
    channel: UdsChannel,
}

impl LocalSttStrategy {
    pub async fn connect(socket: impl AsRef<Path>) -> Result<Self, SttError> {
        let channel = UdsChannel::new(socket)?;
        Ok(Self { channel })
    }

    async fn client(&self) -> Result<SpeechToTextClient<tonic::transport::Channel>, SttError> {
        let ch = self.channel.connect().await?;
        Ok(SpeechToTextClient::new(ch))
    }
}

#[async_trait]
impl Stt for LocalSttStrategy {
    async fn transcribe(
        &self,
        audio_path: &str,
        options: TranscribeOptions,
    ) -> Result<TranscriptStream, SttError> {
        if !Path::new(audio_path).exists() {
            return Err(SttError::AudioFileNotFound(audio_path.to_string()));
        }

        let mut client = self.client().await?;

        let num_speakers: Option<i32> = options
            .num_speakers
            .map(|n| {
                n.try_into().map_err(|_| {
                    SttError::Service(Box::new(Status::invalid_argument(
                        "num_speakers out of range",
                    )))
                })
            })
            .transpose()?;

        let request = TranscribeRequest {
            audio_source: Some(AudioSource::Path(audio_path.to_string())),
            language: options.language.unwrap_or_default(),
            options: Some(ProtoOptions {
                diarization: options.diarization,
                num_speakers,
                initial_prompt: options.initial_prompt.unwrap_or_default(),
            }),
        };

        let stream = client.transcribe(request).await?.into_inner();

        let mapped = stream.map(|result| {
            result
                .map_err(|s| SttError::Service(Box::new(s)))
                .map(|chunk| TranscriptChunk {
                    start_time: chunk.start_time,
                    end_time: chunk.end_time,
                    text: chunk.text,
                    speaker_id: chunk.speaker_id,
                    confidence: chunk.confidence,
                    words: chunk
                        .words
                        .into_iter()
                        .map(|w| Word {
                            start_time: w.start_time,
                            end_time: w.end_time,
                            text: w.text,
                            confidence: w.confidence,
                            speaker_id: w.speaker_id,
                        })
                        .collect(),
                })
        });

        Ok(Box::pin(mapped))
    }
}
