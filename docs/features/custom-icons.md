# Custom bot icons

Issue: [#114](https://github.com/leepokai/Codync/issues/114).

The host owns image import and persistence (`host/src/icons/`). Clients send `setIcon`
with `target: "bot:<bot-id>"` and either standard-base64
`data`, an HTTPS `url`, a `tinted` flag, or `remove: true`. Changes save immediately;
they are separate from a bot's draft settings. Authorized control devices may use it;
pairing-only and screen-only devices may not. The terminal reads its local file before
calling the API; remote clients cannot make the host read a local path.

Agents keep their bundled logos: the host rejects any other target, and the cleanup loop
removes icons that 2.33.2–2.33.6 saved for agents. Their R2 slot is deleted when the cloud
allows it; the local entry goes either way (the slot is overwritten when reused). Clients from
2.33.2–2.33.6 still show an agent Custom icon card, whose changes would fail with "Only bots have
custom icons", so 2.34.0 raises `MIN_APP` to 2.34.0 ([floor history](../reference/compatibility.md#floor-history)).

## Limits and validation

- PNG, JPEG, WebP: 5 MiB input, at most 4096 pixels on either axis, bounded decoder allocation.
- SVG: 256 KiB input, at most 4096 pixels on either axis. Rasterized with resvg;
  no external images/fonts, system fonts, scripts, or network resources.
- EXIF orientation is applied, metadata is discarded, and animations become still images.
- The output fits 256 × 256, preserving aspect ratio and transparency. If PNG exceeds
  48 KiB, its dimensions are halved until it fits.
- URL imports use HTTPS/443, no credentials or proxies, a total timeout, bounded streaming
  reads, at most three redirects, and public IPs pinned after DNS validation for every hop.
- One import per host at a time. The cloud limits mutations to 30/minute/account.
- 128 fixed R2 slots per computer, at most 64 KiB of ciphertext per slot (8 MiB maximum).

## Storage and caches

Normalized PNGs and color modes are cached in the host's SQLite `custom_icons_v1` value.
The host encrypts each PNG with ChaCha20-Poly1305 using a key separated from its identity
box key by SHA-256 and a fresh nonce. AEAD binds it to its slot. R2 receives only a version
byte, nonce and ciphertext; original filenames and source URLs are not retained.

`PUT/GET/DELETE /v1/host/icons/<0..127>` requires a valid host signature, a claimed computer
and an active account. R2 is private; clients receive images over the host's authenticated
loopback or end-to-end encrypted channel. R2 is the remote stored copy; ordinary display
and reconnect use the persistent host cache and do not make R2 GET requests.

Snapshots carry a monotonic revision; clients ignore delayed older replies/events before updating their local caches.

An R2 write must succeed before replacing the local icon. Failed imports keep the previous
cache; color-only changes need no R2 write. Removal deletes R2 first. Slots whose target is no longer
a live bot are swept in small batches, retrying failed cleanup each minute. Account deletion revokes device access and blocks
new writes before removing encrypted images, then finishes unclaiming computers; an R2 error
leaves ownership available for webhook retry. Unclaiming alone retains the encrypted cache
for the same computer identity; it cannot read/write R2 while unclaimed.

The `icons` snapshot/event carries the bounded images once per connection/change rather
than duplicating them in every bot status event. Desktop persists a separate IndexedDB
cache keyed by account and computer; iOS saves the icon map with its per-computer disk
mirror; Android keeps the map in its mirror database. Replacements overwrite the cache
snapshot and removals clear the reference. No client renders arbitrary remote SVG/URLs.

## Live Activity and Dynamic Island

Custom images remain static. Built-in characters keep their existing animation mechanism.
iOS stores content-addressed PNGs in an account-scoped App Group directory (erased on sign-out); ActivityKit carries only the filename.
A small per-computer/bot local pointer lets a running activity follow replacement/removal.
Local state revisions trigger redraws without putting image bytes into APNs payloads.
An offline activity keeps its last cached image until the app reconnects and receives the new snapshot.
Missing custom assets show a still built-in fallback. The desktop island receives the
same normalized image and also renders it statically.

## Deployment and verification

Before deploying cloud code, create private buckets `codync-icons` and `codync-icons-dev`
in the existing Cloudflare account, matching `cloud/wrangler.toml`. Do not enable public
bucket access. Local Wrangler and Worker tests emulate R2 using their environment binding.
Deploy the cloud worker before releasing clients/host; missing storage returns 503 and
leaves previous icons intact. Private relays do not provide this hosted storage service.

Checks: host image/URL/encryption/cache tests, cloud signed R2 isolation/limit tests,
desktop and phone model tests, client builds, and real-app upload, URL, tint, reset,
offline-cache and Live Activity flows. Builds alone do not prove the app flows.

Focused host + local R2 integration check: after `cargo build` in `host/`, run
`CODYNC_E2E_ICONS_ONLY=1 npm run e2e` in `cloud/`. This uses isolated local D1/R2,
a real host and encrypted paired-device requests; it does not verify deployed R2.

The graphical editors show upload and Paste URL actions first. Paste URL reveals the labeled address field and an accessible arrow submit button. The terminal retains its combined file-path-or-URL keyboard field because it has no graphical file picker or icon-only controls.

Computer menus reuse device badges (laptop, Mac mini, Linux, or a generic desktop when unknown). Desktop and Android show connection status as secondary text; iOS keeps native menu typography and selection marks. Terminal UI targets a single host and has no multi-computer filter menu.

Graphical clients require a signed-in app session before selecting a custom image or URL and offer sign-in when needed. Cloud still independently requires a claimed computer and active account. Missing storage routes and server errors are reported as storage availability failures, not authentication failures.
