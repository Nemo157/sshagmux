use crate::error::ErrorExt;
use eyre::Error;
use futures::stream::Stream;
use std::{
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

pub(crate) use tokio::net::{unix::SocketAddr, UnixStream};

pub(crate) struct UnixListener {
    inner: tokio::net::UnixListener,
    unlink: bool,
}

pub(crate) struct Incoming<'a> {
    inner: &'a tokio::net::UnixListener,
}

impl UnixListener {
    #[culpa::throws]
    pub(crate) fn bind(path: impl AsRef<Path>) -> Self {
        Self {
            inner: tokio::net::UnixListener::bind(path)?,
            unlink: true,
        }
    }

    #[culpa::throws]
    pub(crate) fn from_std(listener: std::os::unix::net::UnixListener, unlink: bool) -> Self {
        Self {
            inner: tokio::net::UnixListener::from_std(listener)?,
            unlink,
        }
    }

    pub(crate) fn incoming(&self) -> Incoming {
        Incoming { inner: &self.inner }
    }

    #[culpa::throws]
    pub(crate) fn local_addr(&self) -> SocketAddr {
        self.inner.local_addr()?
    }

    #[culpa::throws]
    pub(crate) fn close(&mut self) {
        if self.unlink {
            self.unlink = false;
            let addr = self.local_addr()?;
            if let Some(path) = addr.as_pathname() {
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
        self.inner
            .poll_accept(cx)
            .map(|r| Some(r.map_err(Error::new)))
    }
}

impl Drop for UnixListener {
    fn drop(&mut self) {
        self.close().log_warn();
    }
}
