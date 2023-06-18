use crate::error::ErrorExt;
use eyre::Error;
use futures::stream::Stream;
use std::{
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

pub(crate) use tokio::net::{unix::SocketAddr, UnixStream};
pub(crate) struct UnixListener(tokio::net::UnixListener);
pub(crate) struct Incoming<'a>(&'a tokio::net::UnixListener);

impl UnixListener {
    #[fehler::throws]
    pub(crate) fn bind(path: impl AsRef<Path>) -> Self {
        Self(tokio::net::UnixListener::bind(path)?)
    }

    #[fehler::throws]
    pub(crate) fn from_std(listener: std::os::unix::net::UnixListener) -> Self {
        Self(tokio::net::UnixListener::from_std(listener)?)
    }

    pub(crate) fn incoming(&self) -> Incoming {
        Incoming(&self.0)
    }

    #[fehler::throws]
    pub(crate) fn local_addr(&self) -> SocketAddr {
        self.0.local_addr()?
    }

    #[fehler::throws]
    pub(crate) fn close(&mut self) {
        let addr = self.local_addr()?;
        if let Some(path) = addr.as_pathname() {
            if path.exists() {
                let _guard = tracing::info_span!("close", path = ?path.display()).entered();
                tracing::debug!("removing socket listener");
                std::fs::remove_file(path)?;
            }
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
        self.close().log_warn();
    }
}
