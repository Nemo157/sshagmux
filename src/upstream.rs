use bytes::{Bytes, BytesMut};
use eyre::Error;
use futures::{
    future::FutureExt,
    stream::{self, FuturesUnordered, Stream, StreamExt},
};
use std::{cell::RefCell, collections::HashMap, future::Future, pin::pin, rc::Rc};
use tracing::Instrument;

use crate::{client::Client, packets::PublicKey};

pub(crate) struct Upstream {
    #[allow(clippy::type_complexity)]
    clients: Rc<RefCell<HashMap<Rc<str>, Rc<RefCell<Client>>>>>,
}

impl Upstream {
    pub(crate) fn new() -> Self {
        Self {
            clients: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub(crate) async fn add(&self, nickname: &str, client: Client) {
        self.clients
            .borrow_mut()
            .insert(Rc::from(nickname), Rc::new(RefCell::new(client)));
    }

    pub(crate) fn list(&self) -> Vec<(String, String)> {
        self.clients
            .borrow()
            .iter()
            .map(|(nickname, client)| {
                (
                    nickname.as_ref().to_owned(),
                    client.borrow_mut().path.clone(),
                )
            })
            .collect()
    }

    pub(crate) fn for_each_client<'a, F, R>(
        &'a self,
        f: impl Fn(Rc<RefCell<Client>>) -> F + 'a,
    ) -> impl Stream<Item = (Rc<str>, R)> + 'a
    where
        F: Future<Output = Result<R, Error>>,
    {
        let f = Rc::new(f);
        async move {
            self.clients
                .borrow()
                .iter()
                .map(|(nickname, client)| {
                    let span = tracing::info_span!("upstream", %nickname);
                    let nickname = nickname.clone();
                    let client = client.clone();
                    let clients = self.clients.clone();
                    let f = f.clone();
                    async move {
                        match f(client).await {
                            Ok(result) => stream::iter(Some((nickname, result))),
                            Err(e) => {
                                match e.downcast_ref::<std::io::Error>() {
                                    Some(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                                        // Remove upstreams that have closed their socket,
                                        // other errors may be transient
                                        clients.borrow_mut().remove(&nickname);
                                        tracing::warn!("removing dead upstream");
                                    }
                                    _ => {
                                        tracing::warn!("error returned from upstream: {e:?}");
                                    }
                                }
                                stream::iter(None)
                            }
                        }
                    }
                    .instrument(span)
                })
                .collect::<FuturesUnordered<_>>()
                .flatten()
        }
        .flatten_stream()
    }

    #[fehler::throws]
    pub(crate) async fn request_identities(&self) -> Vec<PublicKey> {
        self.for_each_client(|client| async move { client.borrow_mut().request_identities().await })
            .flat_map(|(nickname, result)| {
                stream::iter(result.into_iter().map(move |key| {
                    let mut comment = BytesMut::with_capacity(nickname.len() + key.comment.len());
                    comment.extend_from_slice(nickname.as_bytes());
                    comment.extend_from_slice(b": ");
                    comment.extend_from_slice(&key.comment);
                    PublicKey {
                        blob: key.blob,
                        comment: comment.freeze(),
                    }
                }))
            })
            .collect()
            .await
    }

    /// Returns a signature if any upstream gives a success
    pub(crate) async fn sign_request(&self, blob: Bytes, data: Bytes, flags: u32) -> Option<Bytes> {
        pin!(self
            .for_each_client(|client| {
                let blob = blob.clone();
                let data = data.clone();
                async move { client.borrow_mut().sign_request(blob, data, flags).await }
            })
            .filter_map(|(_nickname, signature)| async move { signature }))
        .next()
        .await
    }
}
