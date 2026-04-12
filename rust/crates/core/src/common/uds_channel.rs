use super::error::UdsChannelError;
use hyper_util::rt::TokioIo;
use std::path::{Path, PathBuf};
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

#[derive(Debug, Clone)]
pub struct UdsChannel {
    socket_path: PathBuf,
}

const DUMMY_ENDPOINT: &str = "http://[::]:50051";

impl UdsChannel {
    pub fn new(socket: impl AsRef<Path>) -> Result<Self, UdsChannelError> {
        let path = socket.as_ref();

        if !path.exists() {
            return Err(UdsChannelError::SocketNotFound(
                path.to_string_lossy().into_owned(),
            ));
        }

        Ok(Self {
            socket_path: path.to_path_buf(),
        })
    }

    pub async fn connect(&self) -> Result<Channel, UdsChannelError> {
        let path = self.socket_path.clone();

        let channel = Endpoint::try_from(DUMMY_ENDPOINT)?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = path.clone();
                async move { UnixStream::connect(&path).await.map(TokioIo::new) }
            }))
            .await?;

        Ok(channel)
    }
}
