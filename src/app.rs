use eyre::{eyre, Error, WrapErr as _};
use futures::{
    future::{FutureExt, Shared},
    stream::{StreamExt as _, TryStreamExt as _},
};
use listenfd::ListenFd;
use std::{cell::RefCell, future::Future, path::PathBuf, pin::Pin, rc::Rc};
use tracing::Instrument;

use crate::{
    client::Client,
    error::ErrorExt as _,
    net, server,
    upstreams::{Upstream, Upstreams},
};

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
    /// Also forward any add-identity requests to this server, these can only be forwarded to one
    /// server so only the latest (lowest in `list upstreams`) will have it forwarded
    #[clap(long)]
    forward_adds: bool,
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
    pub(crate) path: RefCell<Option<String>>,
    pub(crate) upstreams: Upstreams,
    pub(crate) shutdown: Shared<Pin<Box<dyn Future<Output = ()>>>>,
}

impl Context {
    pub(crate) fn new(shutdown: impl Future<Output = ()> + 'static) -> Self {
        Self {
            path: RefCell::new(None),
            upstreams: Upstreams::new(),
            shutdown: Box::pin(shutdown).boxed_local().shared(),
        }
    }
}

impl App {
    #[culpa::throws]
    pub(crate) async fn run(self, context: Rc<Context>) {
        tracing::debug!(%self, "starting app");

        match self {
            Self::Daemon(daemon) => daemon.run(context).await?,
            Self::AddUpstream(add_upstream) => add_upstream.run().await?,
            Self::List { list } => list.run().await?,
        }
    }
}

impl Daemon {
    #[culpa::throws]
    pub(crate) async fn run(self, context: Rc<Context>) {
        let mut listener = if self.systemd {
            tracing::info!("getting systemd socket");
            let listener = ListenFd::from_env()
                .take_unix_listener(0)?
                .ok_or_else(|| eyre!("missing systemd socket"))?;
            listener.set_nonblocking(true)?;
            net::UnixListener::from_std(listener, false)?
        } else {
            let bind_address = self.bind_address.unwrap();
            net::UnixListener::bind(bind_address)?
        };

        let path = listener.local_addr().ok().and_then(|addr| {
            addr.as_pathname()
                .and_then(|path| path.to_str().map(|s| s.to_owned()))
        });
        if let Some(path) = &path {
            tracing::info!("bound to {}", path);
        }
        *context.path.borrow_mut() = path;

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
    #[culpa::throws]
    pub(crate) async fn run(self) {
        let Self { path, forward_adds } = self;
        let path = Rc::from(path);
        let client = Client::new(std::env::var("SSH_AUTH_SOCK")?);
        client.add_upstream(Upstream { path, forward_adds }).await?;
    }
}

impl List {
    #[culpa::throws]
    pub(crate) async fn run(self) {
        let client = Client::new(std::env::var("SSH_AUTH_SOCK")?);
        match self {
            Self::Identities => {
                for key in client.request_identities().await? {
                    dbg!(key);
                }
            }
            Self::Upstreams => {
                for upstream in client.list_upstreams().await? {
                    if upstream.forward_adds {
                        println!("{} (add identities forwarded)", upstream.path);
                    } else {
                        println!("{}", upstream.path);
                    }
                }
            }
        }
    }
}

impl std::fmt::Display for App {
    #[culpa::throws(std::fmt::Error)]
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
    #[culpa::throws(std::fmt::Error)]
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
    #[culpa::throws(std::fmt::Error)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) {
        write!(f, "add-upstream")?;
        write!(f, " {:?}", self.path)?;
    }
}

impl std::fmt::Display for List {
    #[culpa::throws(std::fmt::Error)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) {
        write!(f, "list")?;
        match self {
            Self::Identities => write!(f, " identities")?,
            Self::Upstreams => write!(f, " upstreams")?,
        }
    }
}
