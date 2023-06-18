&mdash;An **SSH** **Ag**ent **Mu**ltiple**x**er&mdash;

[![license-badge][]][license] [![rust-version-badge][]][rust-version]

`sshagmux` allows `ssh` to seamlessly use identities from multiple upstream ssh-agents.

The primary usecase this was designed for is a development server that runs a persistent `tmux` session and is connected to by multiple devices with ssh-agent forwarding.
Without multiplexing the best that can be done is replace the socket used by the new forwarded socket on connection, similar to what is described at <https://werat.dev/blog/happy-ssh-agent-forwarding/>.

The problem with this is when you have multiple devices connected to the same session, and switch back and forth between them, if your identities are protected by security-keys then you have to go to the most recently used device to interact and verify the signing request.
By multiplexing to all forwarded agents, we will allow whichever one you are currently at to service the request.

# Setup

There are some files in `configs` showing one example setup, these use paths assuming you have installed via `cargo install --git https://github.com/Nemo157/sshagmux`.

The `sshagmux.{service,socket}` should be installed as user units, e.g. at `$XDG_CONFIG_DIRS/systemd/user` and then set to autostart: `systemd --user --enable --now sshagmux.socket`.
This auto starts the multiplexer for your session when accessed, and will persist it until you logout of all sessions, so for the example usecase of a persistent `tmux` session it will survive reconnections.

The `ssh.rc` should be installed at `~/.ssh/rc`, this is run by `sshd` automatically whenever you create a new connection to the machine.
It detects whether the connection has a forwarded agent and registers it to `sshagmux` as a new upstream.

You will also have to ensure you have `SSH_AUTH_SOCK="${XDG_RUNTIME_DIR}/ssh-agent.socket"`, e.g. by setting this in your profile.
<!-- TODO: maybe `~/.config/environment.d`? -->

After that, any `ssh` use should automatically just work to use any forwarded agent.

# Rust Version Policy

This crate only supports the current stable version of Rust.

# License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you shall be dual licensed as above, without any additional terms or conditions.

[license-badge]: https://img.shields.io/badge/license-MIT/Apache--2.0-blue.svg?style=flat-square
[license]: #license
[rust-version-badge]: https://img.shields.io/badge/rust-latest%20stable-blueviolet.svg?style=flat-square
[rust-version]: #rust-version-policy
