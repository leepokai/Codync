# Security Policy

Codync is a remote control for coding agents running on your computer. Read the trust model below before you report anything: most reports we get describe intended behavior.

## Trust model

- **The computer owner is trusted.** Anything that already runs as the user on the host computer (local apps, shells, files in `~/.codync`) is outside Codync's boundary.
- **A paired device is trusted like the owner.** A device is added only with the owner's consent on the computer: by scanning a one-time pairing code, or by approving an account access request. Once paired, it is meant to have full control of the computer. It can run coding agents (they execute shell commands, by default without asking), change any bot's configuration (agent, launch command, working folder, permission mode), and view and control the screen. A paired device can run code on the computer **by design**. That is the product.
- **Everyone else is untrusted.** This includes the network, the Cloudflare relay (it forwards end-to-end encrypted traffic it cannot read), the push relay, unpaired devices, and anyone holding an expired or used pairing code.

## In scope

Something that crosses one of these boundaries:

- An unpaired party pairing, connecting, or calling any host method.
- The relay, push relay or network reading or forging end-to-end encrypted traffic or push content.
- A device keeping access after it was revoked or its lease expired.
- A paired device calling a method that is restricted to the computer itself (the loopback-only list in `host/src/api/devices.rs`).
- A website or other remote content reaching the loopback API (`127.0.0.1:19222`) without the token.
- Secrets (tokens, keys, voice API keys) leaking off the computer.

## Out of scope

We close these without a reply:

- **"A paired device can run commands on the host."** This covers it through bot launch commands, agent prompts, permission modes, working folders, the screen, or any other route. See the trust model.
- Attacks that require a compromised computer, a compromised paired device, or physical access to an unlocked one.
- What a coding agent does when you prompt it, including prompt injection into an agent you chose to run. Report those to the agent's vendor.
- Self-hosted or third-party relay deployments you configured yourself.
- Missing hardening with no exploit: headers, rate limits beyond those documented, dependency versions with no reachable vulnerable path, and so on.
- Findings from scanners or AI tools that you did not reproduce yourself.

## How to report

Use [GitHub private vulnerability reporting](https://github.com/leepokai/Codync/security/advisories/new). Don't open a public issue. Reports sent by email aren't tracked.

A report must include:

1. The Codync version you tested (a released build, not a guess about `main`).
2. Which trust boundary from this page it crosses, and the attacker's starting position.
3. Steps you ran that reproduce it, plus the actual output or a recording. Pasted source code alone isn't a reproduction.

Reports without a working reproduction, or that describe behavior listed above as out of scope, will be closed.

## CVEs and advisories

We publish a GitHub Security Advisory, and request a CVE through GitHub, only for issues we have confirmed as in scope and fixed. Severity is our call. Please don't request a CVE for Codync from another CNA. For a confirmed issue we credit you in the advisory unless you ask us not to.

Codync is free, open source, and maintained by one person. There is no bug bounty.

## Supported versions

Only the latest release gets security fixes. The desktop app and host update themselves.
