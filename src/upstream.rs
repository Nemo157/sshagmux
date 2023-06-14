use bytes::BytesMut;
use eyre::Error;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tracing::Instrument;

use crate::{client::Client, packets::PublicKey};

pub(crate) struct Upstream {
    clients: Mutex<HashMap<String, Client>>,
}

impl Upstream {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add(&self, nickname: String, client: Client) {
        self.clients.lock().await.insert(nickname, client);
    }

    #[fehler::throws]
    pub(crate) async fn request_identities(&self) -> Vec<PublicKey> {
        let mut keys = Vec::new();
        for (nickname, client) in self.clients.lock().await.iter_mut() {
            keys.extend(
                client
                    .request_identities()
                    .instrument(tracing::info_span!("upstream", nickname))
                    .await?
                    .into_iter()
                    .map(|key| {
                        let mut comment =
                            BytesMut::with_capacity(nickname.len() + key.comment.len());
                        comment.extend_from_slice(nickname.as_bytes());
                        comment.extend_from_slice(b": ");
                        comment.extend_from_slice(&key.comment);
                        PublicKey {
                            blob: key.blob,
                            comment: comment.freeze(),
                        }
                    }),
            )
        }
        keys
    }
}
