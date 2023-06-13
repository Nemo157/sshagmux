use eyre::Error;

use tokio::net::UnixStream;

#[fehler::throws]
pub(crate) async fn handle(_stream: UnixStream) {
    tracing::info!("new connection");
}
