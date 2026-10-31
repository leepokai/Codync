# Codync Cloud: machine and sandbox providers

Survey for [Codync Cloud](codync-cloud.md), 2026-10-24. Facts come from each provider's own docs,
pricing and terms pages fetched that day, unless marked *(Upstash)*: those come from Upstash's
vendor-written comparison ([AI agent sandboxes compared, Sept 2026](https://upstash.com/blog/ai-agent-sandbox-providers-compared-2026))
and were not confirmed first-hand. Terms readings are not legal advice. Prices change; recheck before
deciding.

## What Codync needs from a machine

One persistent Linux computer per user that runs `codync-host` for months: a disk that survives
restarts (repos, `~/.codync`, agent sign-ins), no cap on how long it runs, a cheap way to sleep when
idle and wake within seconds when the phone writes, outbound HTTPS/WSS, terms that let Codync run it for
its customers, and ideally a region near Taiwan.

## How these services isolate machines

| Technology | What it is | Who uses it |
|---|---|---|
| **Firecracker** | AWS's open-source microVM monitor (built for Lambda and Fargate): each sandbox is a small VM with its own kernel, boots in well under a second, and can be snapshotted with its memory, which is how "pause and resume in about a second" works | InstaBox, Fly Machines, Fly Sprites, Vercel Sandbox, E2B, Tensorlake *(Upstash, with Cloud Hypervisor)* |
| **Cloud Hypervisor** | Rust VM monitor from the same family (rust-vmm); full VMs with their own kernel | exe.dev, Northflank with Kata *(Upstash)*, Tensorlake *(Upstash)* |
| Other microVM / KVM | own kernel per sandbox, hypervisor not named | Blaxel, Runloop, Namespace *(Upstash)*, Koyeb *(Upstash)*, Freestyle *(Upstash)*, Cloudflare Sandbox (a container inside its own VM) |
| **gVisor** | Google's user-space kernel: intercepts system calls, shares the host kernel; not a VM | Modal (default; VMs optional), Beam *(Upstash)*, Northflank option *(Upstash)* |
| Containers | shared host kernel, namespaces only | Daytona by default (VM class available), Upstash Box *(Upstash)* |

For Codync isolation is table stakes (one user per machine); what decides is persistence, run limits,
sleep/wake and terms below.

## Agent-machine and sandbox services

| Provider | Isolation | Disk kept | Run limit | Sleep and wake | Price, 2 vCPU-ish | Terms for serving our users | Asia region |
|---|---|---|---|---|---|---|---|
| **InstaBox** (InsForge) | Firecracker | yes, root + `/data` | none | pause keeps memory; resume 8.3 s measured (dashboard), API or public-port visit wakes | 2 vCPU/4 GiB $15.3 always on; Team $500/month floor, Pro $20 (≤5 machines) | no reselling access; terms cover end users of your services (§4c): confirm | no (US) |
| **Fly Machines** | Firecracker | volume only; root disk reset on cold start | none | suspend keeps memory (≤2 GB), resume "hundreds of ms"; Fly proxy wakes on any request; stop/start otherwise | shared-cpu-2x 2 GB $13.4 (Virginia) / $17.5 (Tokyo) + $0.15/GB volume | "internal use" boilerplate, but Fly documents one app per user for user environments; MausBot sells this on Fly | Tokyo, Singapore |
| **Fly Sprites** | Firecracker (hardware microVM) | yes, 100 GB | none, but sleeps after ~30 s idle; outbound sockets don't count | wake on URL, 0.1–2 s; keep awake with the Tasks API | usage-based CPU/RAM; idle ≈ $0.02/GB·month cold storage | same Fly terms | region not selectable |
| **Vercel Sandbox** | Firecracker, own kernel, Ubuntu 26.04 | snapshot on stop (30-day default expiry, can be off) | 24 h per session (Pro), 45 min (Hobby) | resume through the SDK; wake by URL unconfirmed | active CPU $0.128/h + $0.0212/GB·h memory (~$71/month always on) | "make the Services available to any third party" forbidden | Tokyo, Singapore, Seoul, Hong Kong |
| **E2B** | Firecracker | paused sandboxes kept forever, free | 1 h (Hobby) / 24 h (Pro $150/month) continuous | resume ~1 s, auto-resume on traffic, lifecycle webhooks | $0.0504/vCPU·h + $0.0162/GiB·h | forbidden unless authorized in writing | no |
| **Blaxel** | microVM | yes; sandboxes expire after 7/30 days on low tiers | none | standby after ~15 s without connections, resume <25 ms, preview URL wakes | memory $0.0414/GB·h | **end users explicitly allowed** (§6) | no |
| **Boat** (boat.dev; status page on ascii.dev, terms name Dedale; MausBot's sponsor; not the same as boxd.sh) | full Ubuntu VM with root, systemd, Docker, desktop | a stopped sandbox keeps its disk **free for 14 days** (resume and fork use the newest snapshot); longer needs a saved snapshot (10 free, then $1.70/month each) | no limit; auto-stop 1 h by default, up to 30 days or off | no pause (memory not kept): stop = snapshot the disk, resume = reboot in "a few seconds" for any amount of data (lazy hydration), enabled systemd services start again; each create/fork/resume counts as a start: 12/min, 60/h, 200/day on the $20 plan up to 90/600/2,400 on $2,000, raised for committed accounts | small 2 vCPU/4 GB/12 GB $0.018/h, default 4/8/50 $0.036/h ($26/month always on); plans $20/$100/$500/$2,000 a month paid back as compute; their own guide: $1–5/month per typical stop-and-resume user | **TOS-8 explicitly allows giving your users their own sandboxes and charging for it, no approval needed**; not as a plain VM/compute resale | EU only (Germany, Finland, France) |
| **exe.dev** | Cloud Hypervisor, systemd | yes, NVMe + streamed backup | none | always on | $15/month for a 2 vCPU/4 GB pool | resale explicitly forbidden; enterprise only | Tokyo, Singapore |
| Morph Cloud | unconfirmed | snapshots on pause | none | wake on HTTP/SSH | ~$73/month always on | terms page not found | unconfirmed |
| Daytona | container default, VM class | kept on stop | auto-stop can be off | start/stop/archive | ~$122/month | "commercialize the Services" forbidden; low tiers restrict outbound traffic | no |
| Modal | gVisor (VM option) | no (snapshots only) | 24 h | none | ~$174/month | no direct third-party access | — |
| Cloudflare Containers / Sandbox | container in its own VM | not by default (fresh disk on every start); kept through **container snapshots** (beta: filesystem only, ≤20 GB, tied to the image version, 30-day TTL refreshed on restore) or **`DirectoryBackup` to R2** (survives image updates, kept until deleted); R2 FUSE mounts are slow and not fully POSIX | none, but hosts restart irregularly (SIGTERM, up to 15 min to exit) | a Durable Object starts and stops each container: sleep = back up and stop, wake = start from the backup (cold start 1–3 s plus restore; no memory kept) | memory ~$6.6/GiB·month while running; custom 1 vCPU/3 GiB ~$20 + CPU always on, ~$6 asleep; R2 $0.015/GB·month | no selling access without written permission | automatic placement |
| Runloop | microVM ("custom hypervisor") | suspend on Pro ($250/month) | 1 h default lifetime | suspend/resume | ~$231/month | "for the benefit of a third party" forbidden | — |
| Namespace devbox | microVM on own bare metal *(Upstash)* | yes | 3/5/24 h per session | — | ~$175/month | internal, non-commercial use | — |
| Upstash Box | container *(Upstash)* | yes, 10 GB | keep-alive on paid plans | — | $8 + active CPU | no resale without permission | US only |
| Railway sandboxes / agent VMs | unconfirmed | agent VMs keep disk | none | sleep keeps disk | VM memory $50/GB·month | no reselling compute | Singapore |
| GitHub Codespaces | VMs | yes | 12 h max lifetime | — | $0.18/h | repository development only | — |
| Cube | unconfirmed | yes | none | manual pause | ~$14.5/month (2 CPU/4 GB) | no reselling raw compute; no public API | unconfirmed |
| Docker Sandboxes | microVM per sandbox (Docker docs) | — | local product (macOS/Windows), not a cloud | — | — | — | — |

Not covered first-hand: Tensorlake, Freestyle, Koyeb (no volumes in Asia), Northflank (Kata + Cloud
Hypervisor or gVisor, exec API, CSA available), Beam, boxd.sh (a VM per task, persistent disk).

## Cloudflare's own open-source takes (read 2026-10-24)

**[cloudflare/computer](https://github.com/cloudflare/computer)** (MIT, ~9.7k stars, "give your agent a
computer"). The files live in a Durable Object: a virtual filesystem in the object's SQLite storage
(`@cloudflare/dofs`, ~10 GB per workspace, the object's storage cap) is the source of truth. A container
is disposable compute: its `computerd` daemon mounts a mirror of that filesystem over FUSE and syncs over a
capnweb WebSocket. Sync is per command: the object pushes before each `exec` and pulls what the command
wrote after it finishes; container-initiated pushes are listed as future work, and the docs call the
durability gaps deferred. The filesystem, the isolate JavaScript backend and git are marked stable; the
container backend, `computerd` and the sync protocol are experimental ("unsuitable for production").
Speed: about 2× slower than ext4 for a full `npm install`, 17–40× slower for 64 MiB reads and copies,
faster than disk on some metadata-heavy work; `node_modules`-style paths can stay local-only.

Fit for Codync: the idea (storage owned by a Durable Object, compute that comes and goes) matches our
relay's per-computer object, but the sync model is built for agents that run commands, not for a host
that runs for months and writes its SQLite database and agent files continuously. Not usable for
Codync Cloud until container-initiated sync is stable; worth watching.

**[cloudflare/moltworker](https://github.com/cloudflare/moltworker)** (runs the OpenClaw gateway, a
resident agent daemon like `codync-host`, in a Cloudflare Sandbox container; last commit 2026-03-29). It
is the closest template for us: a `standard-1` container (½ vCPU, 4 GiB) costs ~$34.50/month always on
(memory ~$26, CPU at ~10% ~$2, disk ~$1.50, Workers Paid $5), or ~$5–6 of compute for 4 h a day.
Persistence is backup/restore to R2: restore at start, a cron syncs the config every 5 minutes, plus a
"Backup now" button; anything newer than the last backup is lost on a restart. It never sleeps by
default "because cold starts take 1–2 minutes". For Codync Cloud on Cloudflare that cold start is the
number to beat: measure our own image before choosing it.

## Plain VMs and VPS (no memory-keeping pause)

| Provider | 2 vCPU / 4 GB | Stopped machine billed | Terms for serving our users | Taiwan / nearby |
|---|---|---|---|---|
| Google Cloud e2-medium | $28.3 (Taiwan); ~$12.7 on 3-year commit | disk and IP only | your own "Customer Application" allowed, bare resale not | Taiwan (asia-east1), Tokyo, Singapore |
| AWS EC2 t4g.medium | $28.4 (Taipei) | disk only | end users allowed, bare resale not; 5 vCPU default quota | Taipei (opt-in), Tokyo, Singapore |
| Azure B2als_v2 | $32.2 (Taiwan North) | only when deallocated | explicit managed-service clause | Taiwan North, Tokyo, Singapore |
| DigitalOcean | $24 | yes | **resale to end users explicitly allowed**, partner program | Singapore |
| Hetzner | €6–27 (small sizes out of stock) | yes | third-party use allowed, full liability | Singapore |
| Vultr / Linode | $20 / $24 | yes | resale needs permission / forbidden | Tokyo, Osaka, Singapore |
| Oracle Cloud | Always Free A1 (reclaimed when idle 7 days) | — | internal business use only | Tokyo, Osaka, Singapore |

## Reading for Codync

- Firecracker is the common base of the services that can pause a machine with its memory and wake it in
  about a second; that is the property that makes a sleeping cloud computer cheap and still feel
  always on.
- Most Firecracker sandboxes are built for short jobs: session caps (Vercel 24 h, E2B 24 h, Modal 24 h,
  Namespace, Runloop) or expiring sandboxes (Blaxel low tiers) rule them out for a computer that lives
  for months.
- Cloudflare Containers can keep a computer's files after all: back up `/data` to R2 with
  `DirectoryBackup` when it sleeps (and on SIGTERM, and every ~15 min while busy), restore on the next
  start; container snapshots are faster but beta, tied to the image and expire after 30 idle days. What
  it gives up: every wake is a cold start without memory (Cloudflare's own moltworker stays awake because
  its cold starts took 1–2 minutes), a host restart loses work since the last
  backup, R2 FUSE is unfit for git or SQLite. What it gives: the same account and platform as our relay,
  so the computer's Durable Object can start, stop and back up its container itself, with no other
  provider in the loop.
- **Boat** is the cheapest sleeping option with the clearest terms: disk free while stopped (up to 14 days), so a user
  awake 1.7 h a day costs about $0.93/month on `small`, and its terms invite per-user products. Its costs: EU only, every
  wake is a reboot (seconds, unmeasured for our image), and the account-wide daily start limit caps how many wakes a day
  we can sell before a bigger plan or a committed deal.
- Left for a persistent, sleeping computer: **Fly Machines** (Asia regions, wake by request, a proven
  per-user pattern), **InstaBox** (persistent root disk, cheapest units, US only, $500 floor) and
  **Cloudflare Containers + R2 backups** (our own platform, cold wakes, more to build), with Blaxel (terms
  allow it) and exe.dev (cheapest always-on, no resale) as outsiders.
- Big clouds and VPSs work for an always-on machine with standard terms, but sleeping there means a full
  stop and boot.
