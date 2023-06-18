use bytes::Bytes;
use eyre::Error;
use futures::{
    future::{self, FutureExt},
    stream::{self, FuturesUnordered, Stream, StreamExt},
};
use std::{cell::RefCell, collections::HashSet, future::Future, pin::pin, rc::Rc};

use crate::{client::Client, packets::PublicKey};

pub(crate) struct Upstream {
    #[allow(clippy::type_complexity)]
    clients: Rc<RefCell<HashSet<Rc<Client>>>>,
}

impl Upstream {
    pub(crate) fn new() -> Self {
        Self {
            clients: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    pub(crate) async fn add(&self, client: Client) {
        self.clients.borrow_mut().insert(Rc::new(client));
    }

    pub(crate) fn list(&self) -> Vec<String> {
        self.clients
            .borrow()
            .iter()
            .map(|client| client.path.clone())
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
                .iter()
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
                                        clients.borrow_mut().remove(&client);
                                        tracing::warn!(path = client.path, "removed dead upstream");
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
                .collect::<FuturesUnordered<_>>()
                .flatten()
        }
        .flatten_stream()
    }

    #[fehler::throws]
    pub(crate) async fn request_identities(&self) -> Vec<PublicKey> {
        self.for_each_client(|client| async move { client.request_identities().await })
            .flat_map(stream::iter)
            .collect::<HashSet<_>>()
            .await
            .into_iter()
            .collect()
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
