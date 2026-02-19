use super::{SpeechToText, SttError, TranscribeOptions, TranscriptChunk, TranscriptStream, Word};
use async_trait::async_trait;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tokio_stream::StreamExt;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

pub mod proto {
    tonic::include_proto!("speech");
}

use proto::{
    TranscribeOptions as ProtoOptions, TranscribeRequest,
    speech_to_text_client::SpeechToTextClient, transcribe_request::AudioSource,
};

pub struct LocalSttStrategy {
    socket_path: String,
}

impl LocalSttStrategy {
    pub async fn connect(socket_path: impl Into<String>) -> Result<Self, SttError> {
        let path = socket_path.into();

        if !std::path::Path::new(&path).exists() {
            return Err(SttError::SocketNotFound(path));
        }

        Ok(Self { socket_path: path })
    }

    async fn client(&self) -> Result<SpeechToTextClient<tonic::transport::Channel>, SttError> {
        let path = self.socket_path.clone();

        let channel = Endpoint::try_from("http://localhost")?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = path.clone();
                async move {
                    // TokioIo bridges tokio's AsyncRead/AsyncWrite to hyper's traits
                    UnixStream::connect(&path).await.map(TokioIo::new)
                }
            }))
            .await?;

        Ok(SpeechToTextClient::new(channel))
    }
}

#[async_trait]
impl SpeechToText for LocalSttStrategy {
    async fn transcribe(
        &self,
        audio_path: &str,
        options: TranscribeOptions,
    ) -> Result<TranscriptStream, SttError> {
        if !std::path::Path::new(audio_path).exists() {
            return Err(SttError::AudioFileNotFound(audio_path.to_string()));
        }

        let mut client = self.client().await?;

        let num_speakers: i32 = options.num_speakers.try_into().map_err(|_| {
            SttError::Service(tonic::Status::invalid_argument("num_speakers out of range"))
        })?;

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
                .map_err(SttError::Service)
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
                        })
                        .collect(),
                })
        });

        Ok(Box::pin(mapped))
    }
}
