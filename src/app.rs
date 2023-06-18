use eyre::{eyre, Error, WrapErr as _};
use futures::{
    future::{FutureExt, Shared},
    stream::{StreamExt as _, TryStreamExt as _},
};
use listenfd::ListenFd;
use std::{future::Future, path::PathBuf, pin::Pin, sync::Arc};
use tracing::Instrument;

use crate::{client::Client, error::ErrorExt as _, net, server, upstream::Upstream};

#[derive(Debug, clap::Parser)]
#[command(version, disable_help_subcommand = true)]
pub(crate) enum App {
    Daemon(Daemon),
    AddUpstream(AddUpstream),
    List {
        #[command(subcommand)]
        list: List,
    },
}

/// Start up as a daemon
#[derive(Debug, clap::Parser)]
pub(crate) struct Daemon {
    #[arg(long, short('a'), required_unless_present = "systemd")]
    bind_address: Option<PathBuf>,
    #[arg(long, short, conflicts_with = "bind_address")]
    systemd: bool,
}

/// Connect to the instance at `SSH_AUTH_SOCK` and tell it to add `path` as an upstream server
#[derive(Debug, clap::Parser)]
pub(crate) struct AddUpstream {
    path: String,
}

/// Connect to the instance at `SSH_AUTH_SOCK` and list items from it
#[derive(Debug, clap::Parser)]
pub(crate) enum List {
    /// List identities (like `ssh-add -l`)
    Identities,
    /// List upstreams
    Upstreams,
}

pub(crate) struct Context {
    pub(crate) upstream: Upstream,
    pub(crate) shutdown: Shared<Pin<Box<dyn Future<Output = ()>>>>,
}

impl Context {
    pub(crate) fn new(shutdown: impl Future<Output = ()> + 'static) -> Self {
        Self {
            upstream: Upstream::new(),
            shutdown: Box::pin(shutdown).boxed_local().shared(),
        }
    }
}

impl App {
    #[fehler::throws]
    pub(crate) async fn run(self, context: Arc<Context>) {
        tracing::info!(%self, "starting app");

        match self {
            Self::Daemon(daemon) => daemon.run(context).await?,
            Self::AddUpstream(add_upstream) => add_upstream.run().await?,
            Self::List { list } => list.run().await?,
        }
    }
}

impl Daemon {
    #[fehler::throws]
    pub(crate) async fn run(self, context: Arc<Context>) {
        let mut listener = if self.systemd {
            let listener = ListenFd::from_env()
                .take_unix_listener(0)?
                .ok_or_else(|| eyre!("missing systemd socket"))?;
            listener.set_nonblocking(true)?;
            net::UnixListener::from_std(listener)?
        } else {
            net::UnixListener::bind(self.bind_address.unwrap())?
        };

        let mut next_id = 0;
        listener
            .incoming()
            .take_until(context.shutdown.clone())
            .map_err(|e| e.wrap_err("failed to accept connection"))
            .try_for_each_concurrent(None, |(stream, _addr)| {
                let connection_id = next_id;
                next_id += 1;
                let context = context.clone();
                async move {
                    if let Err(e) = server::handle(stream, context).await {
                        tracing::warn!("{e:?}");
                    }
                    Ok(())
                }
                .instrument(tracing::info_span!("connection", connection_id))
            })
            .await?;

        listener
            .close()
            .context("could not close unix listener")
            .log_warn();
    }
}

impl AddUpstream {
    #[fehler::throws]
    pub(crate) async fn run(self) {
        let client = Client::new(std::env::var("SSH_AUTH_SOCK")?);
        client.add_upstream(self.path).await?;
    }
}

impl List {
    #[fehler::throws]
    pub(crate) async fn run(self) {
        let client = Client::new(std::env::var("SSH_AUTH_SOCK")?);
        match self {
            Self::Identities => {
                for key in client.request_identities().await? {
                    dbg!(key);
                }
            }
            Self::Upstreams => {
                for path in client.list_upstreams().await? {
                    println!("{path}");
                }
            }
        }
    }
}

impl std::fmt::Display for App {
    #[fehler::throws(std::fmt::Error)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) {
        write!(f, "sshagmux")?;
        match self {
            Self::Daemon(daemon) => write!(f, " {daemon}")?,
            Self::AddUpstream(add_upstream) => write!(f, " {add_upstream}")?,
            Self::List { list } => write!(f, " {list}")?,
        }
    }
}

impl std::fmt::Display for Daemon {
    #[fehler::throws(std::fmt::Error)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) {
        write!(f, "daemon")?;
        if self.systemd {
            write!(f, " --systemd")?;
        } else {
            write!(
                f,
                " --bind-address={:?}",
                self.bind_address.as_ref().unwrap().display()
            )?;
        }
    }
}

impl std::fmt::Display for AddUpstream {
    #[fehler::throws(std::fmt::Error)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) {
        write!(f, "add-upstream")?;
        write!(f, " {:?}", self.path)?;
    }
}

impl std::fmt::Display for List {
    #[fehler::throws(std::fmt::Error)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) {
        write!(f, "list")?;
        match self {
            Self::Identities => write!(f, " identities")?,
            Self::Upstreams => write!(f, " upstreams")?,
        }
    }
}
