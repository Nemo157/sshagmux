use eyre::{Error, WrapErr};
use futures::stream::Stream;
use std::{
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

pub(crate) use tokio::net::{unix::SocketAddr, UnixStream};
pub(crate) struct UnixListener(Option<PathBuf>, tokio::net::UnixListener);
pub(crate) struct Incoming<'a>(&'a tokio::net::UnixListener);

impl UnixListener {
    #[fehler::throws]
    pub(crate) fn bind(path: impl AsRef<Path>) -> Self {
        Self(
            Some(path.as_ref().to_owned()),
            tokio::net::UnixListener::bind(path)?,
        )
    }

    pub(crate) fn incoming(&self) -> Incoming {
        Incoming(&self.1)
    }

    #[fehler::throws]
    pub(crate) fn close(&mut self) {
        if let Some(path) = self.0.take() {
            std::fs::remove_file(&path).with_context(|| format!("at path {path:?}"))?;
        }
    }
}

impl Stream for Incoming<'_> {
    type Item = Result<(UnixStream, SocketAddr), Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_accept(cx).map(|r| Some(r.map_err(Error::new)))
    }
}

impl Drop for UnixListener {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
