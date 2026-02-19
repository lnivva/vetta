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
    speech_to_text_client::SpeechToTextClient, TranscribeOptions as ProtoOptions, TranscribeRequest,
};

pub struct LocalSttStrategy {
    socket_path: String,
}

impl LocalSttStrategy {
    /// Create a LocalSttStrategy that will connect to a local gRPC speech-to-text service via a UNIX domain socket.
    ///
    /// The provided `socket_path` is converted to a `String` and must refer to an existing filesystem entry;
    /// otherwise the function returns `SttError::SocketNotFound`.
    ///
    /// # Errors
    ///
    /// Returns `SttError::SocketNotFound(path)` when `socket_path` does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::fs::File;
    /// use std::path::PathBuf;
    ///
    /// // create a temporary socket file path for the example
    /// let mut p = std::env::temp_dir();
    /// p.push("example_local_stt_socket.sock");
    /// // ensure the path exists for the example
    /// let _ = File::create(&p).unwrap();
    ///
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// let strategy = rt.block_on(async {
    ///     LocalSttStrategy::connect(p.to_string_lossy()).await.unwrap()
    /// });
    ///
    /// // cleanup
    /// let _ = std::fs::remove_file(p);
    /// ```
    pub async fn connect(socket_path: impl Into<String>) -> Result<Self, SttError> {
        let path = socket_path.into();

        if !std::path::Path::new(&path).exists() {
            return Err(SttError::SocketNotFound(path));
        }

        Ok(Self { socket_path: path })
    }

    /// Create a gRPC SpeechToTextClient connected over the strategy's configured Unix domain socket.
    ///
    /// The returned client speaks to the local STT service via the Unix domain socket path stored in
    /// this strategy. Connection and channel setup errors are propagated as `SttError`.
    ///
    /// # Returns
    ///
    /// A `SpeechToTextClient<tonic::transport::Channel>` connected to the configured socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crate::stt::local::LocalSttStrategy;
    /// # use crate::stt::SttError;
    /// #[tokio::test]
    /// async fn build_client() -> Result<(), SttError> {
    ///     let strategy = LocalSttStrategy::connect("/tmp/stt.sock").await?;
    ///     let _client = strategy.client().await?;
    ///     Ok(())
    /// }
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
    /// Transcribes an audio file using the local gRPC speech-to-text service and returns a stream of transcript chunks.
    ///
    /// If the audio file does not exist, returns `SttError::AudioFileNotFound(path)`. Errors produced by the remote service are returned as `SttError::Service`.
    ///
    /// # Arguments
    ///
    /// * `audio_path` - Path to the audio file to transcribe.
    /// * `options` - Transcription options (language, diarization, number of speakers, initial prompt).
    ///
    /// # Returns
    ///
    /// A stream yielding `Result<TranscriptChunk, SttError>` items; each successful item is a `TranscriptChunk`.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures::StreamExt;
    /// # async fn example(strategy: &crate::stt::local::LocalSttStrategy) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut stream = strategy.transcribe("audio.wav", Default::default()).await?;
    /// while let Some(item) = stream.next().await {
    ///     let chunk = item?;
    ///     println!("{}", chunk.text);
    /// }
    /// # Ok(()) }
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

        let request = TranscribeRequest {
            audio_path: audio_path.to_string(),
            language: options.language.unwrap_or_default(),
            options: Some(ProtoOptions {
                diarization: options.diarization,
                num_speakers: options.num_speakers as i32,
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