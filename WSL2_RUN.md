# Run the client on WSL2

This guide is self-contained so it can be copied to another branch. Run commands
from the repository root inside WSL2. The project uses Rust 1.99.0, Bevy 0.19.1,
and a graphical client enabled by the default `netgraph` feature.

## What failed and what worked

In this WSL2 environment, WSLg exposed both `WAYLAND_DISPLAY=wayland-0` and
`DISPLAY=:0`. The client initially chose Wayland, opened a window, and then
exited with `Io error: Broken pipe (os error 32)`. Running the same client with
Wayland disabled selected WSLg's X11 connection. The window stayed open and the
client printed `ROUND_TRIP_OK` after exchanging messages with the server.

The environment did not expose `/dev/dxg`, so Bevy selected Mesa's `llvmpipe`
software Vulkan adapter. That adapter worked but may render slowly. The X11
change fixes the client startup failure; GPU acceleration depends on Windows
and WSL graphics drivers exposing `/dev/dxg`.

## Run without changing code

This is the quickest option on a branch that still chooses Wayland by default.
Start the server in one WSL terminal:

```sh
cargo run --locked -p server
```

Start each graphical client in a separate WSL terminal. Give every client a
different ID:

```sh
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET cargo run --locked -p client -- --client-id 1
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET cargo run --locked -p client -- --client-id 2
```

The client reads `config/network.toml` by default. Its `server_addr` must point
to the server; the current local default is `127.0.0.1:5000`. A successful
connection prints `ROUND_TRIP_OK`. Stop each process with Ctrl+C.

If the branch has no graphical client or WSLg display, use the headless build:

```sh
cargo run --locked -p client --no-default-features -- --client-id 1 --headless
```

## Apply the WSL2 client change to another branch

The change is in `crates/client/src/main.rs`. Its purpose is to select X11
automatically when running a graphical client under WSL2 with `DISPLAY` set.
It leaves non-WSL Linux and headless runs alone. First, add this field to the
`Args` struct, near `headless`:

```rust
/// Use Wayland on WSL instead of the more reliable WSLg X11 path.
#[arg(long)]
wayland: bool,
```

In `main`, after parsing arguments and loading the configuration, but before
building the Bevy app or starting threads, call:

```rust
#[cfg(all(feature = "netgraph", target_os = "linux"))]
if !args.headless && !args.wayland {
    prefer_wslg_x11();
}
```

Add this function outside `main`:

```rust
#[cfg(all(feature = "netgraph", target_os = "linux"))]
fn prefer_wslg_x11() {
    if std::env::var_os("WSL_DISTRO_NAME").is_none() || std::env::var_os("DISPLAY").is_none() {
        return;
    }
    // winit chooses Wayland whenever either variable is set. This runs before
    // Bevy starts any threads, which is required for safe environment changes.
    unsafe {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
}
```

After applying the change, the usual commands work without the `env -u` prefix:

```sh
cargo run --locked -p server
cargo run --locked -p client -- --client-id 1
```

Use `--wayland` to opt back into Wayland on WSL2. This code assumes the other
branch has the same `netgraph` feature and Clap-based `Args` structure. If its
startup differs, keep the same rule: remove both Wayland environment variables
before Bevy or winit creates the event loop and before other threads start.

## Check the environment

```sh
printf 'DISPLAY=%s WAYLAND_DISPLAY=%s WSL_DISTRO_NAME=%s\n' "$DISPLAY" "$WAYLAND_DISPLAY" "$WSL_DISTRO_NAME"
ls -l /tmp/.X11-unix/X0
ls -l /dev/dxg
```

The first two checks confirm WSLg's X11 display is available. A missing
`/dev/dxg` explains why Bevy uses `llvmpipe`; it does not prevent this client
from running with software rendering. If `DISPLAY` or the X11 socket is absent,
check that WSLg GUI applications are enabled in the WSL installation.

Build checks used for this change:

```sh
cargo fmt --all --check
cargo check --locked -p client
cargo check --locked -p client --no-default-features
cargo clippy --locked -p client --all-targets -- -D warnings
```
