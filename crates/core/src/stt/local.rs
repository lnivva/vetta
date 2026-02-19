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
    /// Create a LocalSttStrategy after verifying the Unix-domain socket path exists.
    ///
    /// If the provided path does not exist, this returns `SttError::SocketNotFound`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crate::stt::local::LocalSttStrategy;
    /// # use crate::stt::SttError;
    /// # async fn example() -> Result<(), SttError> {
    /// let strategy = LocalSttStrategy::connect("/var/run/stt.sock").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(socket_path: impl Into<String>) -> Result<Self, SttError> {
        let path = socket_path.into();

        if !std::path::Path::new(&path).exists() {
            return Err(SttError::SocketNotFound(path));
        }

        Ok(Self { socket_path: path })
    }

    /// Create a gRPC SpeechToText client connected to this strategy's Unix-domain socket.
    ///
    /// The returned client is configured to communicate with the local speech service
    /// through the socket path stored in `self.socket_path`.
    ///
    /// # Errors
    ///
    /// Returns `SttError` if the gRPC endpoint cannot be created or the connection to the
    /// Unix-domain socket fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # use crate::stt::local::LocalSttStrategy;
    /// # use crate::stt::{SttError, TranscribeOptions};
    /// # async fn _example() -> Result<(), SttError> {
    /// let strategy = LocalSttStrategy::connect("/tmp/speech.sock").await?;
    /// let mut client = strategy.client().await?;
    /// // use `client` to call RPCs...
    /// # Ok(())
    /// # }
    /// ```
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
    /// Transcribes a local audio file by calling the configured local gRPC speech-to-text service.
    ///
    /// Validates that `audio_path` exists, sends a transcribe request using the provided `options`,
    /// and returns a stream of transcription chunks produced by the service.
    ///
    /// # Errors
    ///
    /// - Returns `SttError::AudioFileNotFound` if `audio_path` does not exist.
    /// - Returns `SttError::Service` with an `invalid_argument` status if `options.num_speakers` cannot
    ///   be represented as an `i32`.
    /// - Individual stream items propagate service-side errors as `SttError::Service`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_stream::StreamExt;
    /// # async fn example_call(stt: &impl crate::stt::SpeechToText) -> Result<(), crate::stt::SttError> {
    /// let options = crate::stt::TranscribeOptions {
    ///     language: Some("en".to_string()),
    ///     diarization: true,
    ///     num_speakers: 2,
    ///     initial_prompt: None,
    /// };
    ///
    /// let mut stream = stt.transcribe("tests/fixtures/sample.wav", options).await?;
    ///
    /// while let Some(item) = stream.next().await {
    ///     let chunk = item?;
    ///     println!("{}: {}", chunk.start_time, chunk.text);
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
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