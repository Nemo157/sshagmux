use bytes::Bytes;
use eyre::{bail, Error};
use futures::{
    future::{self, FutureExt},
    stream::{self, FuturesOrdered, Stream, StreamExt},
};
use indexmap::{IndexMap, IndexSet};
use std::{cell::RefCell, future::Future, pin::pin, rc::Rc, time::Duration};

use crate::{
    client::Client,
    packets::{PublicKey, Request, Response},
};

#[derive(Debug)]
pub(crate) struct Upstream {
    pub(crate) path: Rc<str>,
    pub(crate) forward_adds: bool,
}

pub(crate) struct Upstreams {
    #[allow(clippy::type_complexity)]
    clients: Rc<RefCell<IndexMap<Rc<str>, Rc<Client>>>>,
}

impl Upstreams {
    pub(crate) fn new() -> Self {
        Self {
            clients: Rc::new(RefCell::new(IndexMap::new())),
        }
    }

    pub(crate) async fn add(&self, client: Client) {
        // We explicitly remove and readd the client to put it at the end of the list
        let mut clients = self.clients.borrow_mut();
        clients.shift_remove(&client.path);
        clients.insert(client.path.clone(), Rc::new(client));
    }

    pub(crate) fn list(&self) -> Vec<Upstream> {
        self.clients
            .borrow()
            .values()
            .map(|client| client.info())
            .collect()
    }

    pub(crate) fn for_each_client<'a, F, R>(
        &'a self,
        f: impl Fn(Rc<Client>) -> F + 'a,
    ) -> impl Stream<Item = R> + 'a
    where
        F: Future<Output = Result<R, Error>>,
    {
        let f = Rc::new(f);
        async move {
            self.clients
                .borrow()
                .values()
                .rev()
                .map(|client| {
                    let client = client.clone();
                    let clients = self.clients.clone();
                    let f = f.clone();
                    async move {
                        match f(client.clone()).await {
                            Ok(result) => stream::iter(Some(result)),
                            Err(e) => {
                                match e.downcast_ref::<std::io::Error>() {
                                    Some(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                        // Remove upstreams that have closed their socket,
                                        // other errors may be transient
                                        clients.borrow_mut().shift_remove(&client.path);
                                        tracing::warn!(path = %client.path, "removed dead upstream");
                                    }
                                    _ => {
                                        tracing::warn!("error returned from upstream: {e:?}");
                                    }
                                }
                                stream::iter(None)
                            }
                        }
                    }
                })
                .collect::<FuturesOrdered<_>>()
                .flatten()
        }
        .flatten_stream()
    }

    #[culpa::throws]
    pub(crate) async fn request_identities(&self) -> Vec<PublicKey> {
        self.for_each_client(|client| async move { client.request_identities().await })
            .flat_map(stream::iter)
            .collect::<IndexSet<_>>()
            .await
            .into_iter()
            .collect()
    }

    #[culpa::throws]
    pub(crate) async fn forward_to_adds(&self, message: Request) -> Response {
        let Some(client) = self
            .clients
            .borrow()
            .values()
            .rev()
            .find(|client| client.forward_adds)
            .cloned()
        else {
            bail!("no client configured to forward adds to")
        };
        client.send(message, Duration::from_secs(1)).await?
    }

    /// Returns a signature if any upstream gives a success
    pub(crate) async fn sign_request(&self, blob: Bytes, data: Bytes, flags: u32) -> Option<Bytes> {
        pin!(self
            .for_each_client(|client| {
                let blob = blob.clone();
                let data = data.clone();
                async move { client.sign_request(blob, data, flags).await }
            })
            .filter_map(future::ready))
        .next()
        .await
    }
}
