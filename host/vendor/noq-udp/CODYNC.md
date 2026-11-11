# noq-udp 1.4.0, patched for Codync

Copied from crates.io `noq-udp` 1.4.0 and used through `[patch.crates-io]` in `host/Cargo.toml`.

The one change: on Apple platforms an IPv4 source address is sent with `IP_PKTINFO`, as upstream
quinn-udp does since quinn-rs/quinn@a2c55b3681. macOS ignores `IP_RECVDSTADDR` as a source
override, so a Mac reachable on several IPv4 addresses answered a QUIC handshake from the routed
address instead of the one the client used; noq then dropped the client's next packets
("discarding packet sent to incorrect interface") and the connection never opened. That broke
codync-network connections into a Mac from a computer that can reach two of its addresses.

Delete this folder and the patch entry once a noq-udp release carries the fix. Until then the host's CI and the
pre-commit hook fail when the patch stops applying (cargo only warns, for example once a newer noq-udp is on
crates.io and `cargo update` takes it): check whether that release has the fix, then delete this folder or copy the
new release here and apply the change again.
