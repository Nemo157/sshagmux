use bytes::BytesMut;
use eyre::Error;
use futures::stream::{self, Stream, StreamExt};
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::Mutex;
use tracing::Instrument;

use crate::{client::Client, packets::PublicKey};

pub(crate) struct Upstream {
    #[allow(clippy::type_complexity)]
    clients: Arc<Mutex<HashMap<Arc<str>, Arc<Mutex<Client>>>>>,
}

impl Upstream {
    pub(crate) fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn add(&self, nickname: &str, client: Client) {
        self.clients
            .lock()
            .await
            .insert(Arc::from(nickname), Arc::new(Mutex::new(client)));
    }

    pub(crate) async fn list(&self) -> Vec<(Arc<str>, String)> {
        stream::iter(self.clients.lock().await.iter())
            .then(|(nickname, client)| async {
                (nickname.clone(), client.lock().await.path.clone())
            })
            .collect()
            .await
    }

    pub(crate) fn for_each_client<'a, F, R>(
        &'a self,
        f: impl Fn(Arc<Mutex<Client>>) -> F + 'a,
    ) -> impl Stream<Item = (Arc<str>, R)> + 'a
    where
        F: Future<Output = Result<R, Error>>,
    {
        let f = Arc::new(f);
        stream::once(async move {
            stream::iter(
                self.clients
                    .clone()
                    .lock_owned()
                    .await
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
                                            clients.lock_owned().await.remove(&nickname);
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
                    .collect::<Vec<_>>(),
            )
            .buffer_unordered(5)
            .flatten()
        })
        .flatten()
    }

    #[fehler::throws]
    pub(crate) async fn request_identities(&self) -> Vec<PublicKey> {
        self.for_each_client(|client| async move { client.lock().await.request_identities().await })
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
}
