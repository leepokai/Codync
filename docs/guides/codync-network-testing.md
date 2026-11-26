# Testing codync-network locally with Lima

Two levels, both on this Mac with the Lima Linux VM `codync-linux` (the Full e2e Linux VM; if it isn't installed, create it with
`limactl create --name codync-linux template://default`):

1. **Crate tests in the VM.** `cargo test` in `host/crates/network` starts two computers in one process over direct
   connections (no relay, no internet). It runs the same on Linux, so it catches Linux-only breakage.
2. **Two real daemons, Mac and VM.** The standalone `codync-network` daemon on the Mac and in the VM, joined into one
   network. This is the closest local run to two real computers.

The `codync-host` level (`host/tests/network_e2e.rs`) is covered at the end.

## Set up the VM

```sh
limactl start codync-linux          # status: limactl list
limactl shell codync-linux
```

The Mac home is mounted read-only in the VM at the same path, so clone the repo instead of building in place:

```sh
git clone --depth 1 --branch dev "file://<path of your checkout on the Mac>" ~/codync
```

Uncommitted changes are not in that clone. Copy them over from the Mac (one `limactl copy` per file, run on the Mac):

```sh
limactl copy host/crates/network/src/rules.rs codync-linux:~/codync/host/crates/network/src/rules.rs
```

Toolchain: the crate follows `host/rust-toolchain.toml`. If `cargo` is missing in the VM, install it with
rustup first. Reuse one target folder across runs:

```sh
export CARGO_TARGET_DIR=$HOME/codync-target-network
```

## Level 1: crate tests in the VM

Inside the VM:

```sh
cd ~/codync/host/crates/network
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The tests use temporary folders and loopback addresses. Nothing leaves the VM.

## Level 2: a Mac daemon and a VM daemon

Build the daemon on each side. The Mac build comes from the `host` workspace:

```sh
# Mac
cd host && cargo build -p codync-network && cd ..
# VM
cd ~/codync/host && cargo build -p codync-network
```

Each daemon needs its own state folder (`--dir`, default `~/.codync-network`). Never point two daemons at one folder.
The flags are taken anywhere on the command line.

Keep each daemon running in its own terminal (the Mac one in a terminal, the VM one in its own `limactl shell`
session).

**Computer 1, on the Mac (the owner):**

```sh
host/target/debug/codync-network --dir ~/codync-net-mac init desk
host/target/debug/codync-network --dir ~/codync-net-mac invite          # codync-network:… (one use, 1 h)
```

**Computer 2, in the VM (joins):**

```sh
~/codync-target-network/debug/codync-network --dir ~/codync-net-vm join 'codync-network:…' lima
```

Then check both directions:

```sh
# Mac
host/target/debug/codync-network --dir ~/codync-net-mac peers                    # both computers listed, one online
host/target/debug/codync-network --dir ~/codync-net-mac send terminal@lima hello  # delivered in the VM
# VM
~/codync-target-network/debug/codync-network --dir ~/codync-net-vm send terminal@desk hello  # delivered on the Mac
```

Below, `codync-network --dir <dir>` stands for the side's own binary and folder as above.

Use `--connector` to choose how the computers reach each other:

| Connector | What to expect here |
| --- | --- |
| `public` (default) | iroh's public relays. Needs internet in the VM. Works even when a direct path doesn't. |
| `private` | No relay at all. Only passes if the two computers can reach each other directly. |

Lima's default NAT networking does not promise a direct route from the VM to the Mac. Whether `private` connects here
is **not yet verified**: try it, and record the result in this doc. If it fails, the cause is the network, not the
code. For a guaranteed LAN, Lima's shared-network setup (`socket_vmnet`) is the next step.

## Things to check

- **Away computer → queued.** Stop the VM daemon, send `terminal@lima` from the Mac, and check the letter waits in the Mac's outbox
  (up to a day). Start the VM daemon again and it arrives.
- **Budgets.** Five messages in a burst, then one every 2 minutes per pair of agents. The same words twice within
  10 minutes are refused. A refused send is the expected result, not a bug.
- **Rules.** `codync-network --dir <dir> rules deny '*@lima' terminal` on the Mac; a send from the VM is refused
  with the reason in the sender's error. Remove it with `rules remove <n>`.
- **Removal.** `codync-network --dir <dir> remove lima` on the Mac; the VM's sends then fail.

## Host-level end to end (optional)

`host/tests/network_e2e.rs` runs two full `codync-host` processes in one network (invites, bot messages across
computers, the view and its events). Run it in the VM the same way, from the `host` workspace:

```sh
cd ~/codync/host && cargo test -p codync-host --test network_e2e
```

Building the whole host in the VM takes a long time the first run. Reuse `CARGO_TARGET_DIR`.

## Clean up

```sh
rm -rf ~/codync-net-mac                              # on the Mac
limactl shell codync-linux -- sh -c 'rm -rf ~/codync-net-vm'   # in the VM
limactl stop codync-linux                            # when finished
```

Do not delete `~/codync-target-network` if you plan to run again; it is the build cache.
