# Codync

A real-time companion for [Claude Code](https://docs.anthropic.com/en/docs/claude-code) — monitor your coding sessions from anywhere, on any Apple device.

> Now you can vibe code with Claude Code while jogging.

[![Download on the App Store](https://img.shields.io/badge/App_Store-iOS-blue?logo=apple)](https://apps.apple.com/tw/app/codync/id6760984418?l=en-GB)
[![Download for macOS](https://img.shields.io/badge/Download-macOS-black?logo=apple)](https://github.com/leepokai/Codync/releases/latest/download/Codync-macOS.dmg)

## Features

- **Dynamic Island & Live Activity** — See session status right on your Lock Screen and Dynamic Island, even when the app is closed
- **macOS Menu Bar** — Instant session overview without leaving your workflow
- **Cross-device Sync** — CloudKit-powered sync between Mac and iPhone, no LAN required
- **Push Notifications** — Get notified when Claude needs your input or finishes a task
- **Hook-driven Detection** — 7 Claude Code hooks provide instant, accurate status with ~20ms latency
- **Zero Configuration** — One-click install, no login, no account, no analytics

## How It Works

1. **macOS app** installs lightweight hook scripts into Claude Code
2. Hooks fire on session events (start, stop, tool use, permission requests, etc.)
3. Session state syncs to iOS via CloudKit in real-time
4. iOS app displays status via Live Activity, Dynamic Island, and push notifications

## Install

**macOS** — Download the [latest DMG](https://github.com/leepokai/Codync/releases/latest/download/Codync-macOS.dmg) and drag to Applications

**iOS** — Install from the [App Store](https://apps.apple.com/tw/app/codync/id6760984418?l=en-GB)

The in-app onboarding wizard will guide you through setup — just open the macOS app and follow the steps.

## Project Structure

| Target | Description |
|---|---|
| `Codync-macOS` | macOS menu bar app — hook server, transcript parsing, CloudKit sync |
| `Codync-iOS` | iOS companion — Live Activity, Dynamic Island, push notifications |
| `CodyncShared` | Shared Swift Package — models, CloudKit logic, theme |
| `CodyncLiveActivity` | iOS Live Activity widget extension |
| `worker/` | Cloudflare Worker — APNs relay for background Live Activity updates |
| `Codync-web/` | Landing page (Next.js) |

## Tech Stack

- Swift 6 strict concurrency
- SwiftUI + `@Observable`
- CloudKit for cross-device sync
- Claude Code hooks (command-based, ~20ms overhead)
- Cloudflare Worker for APNs relay
- Next.js landing page

## Acknowledgments

Inspired by these awesome projects:

- [chowder-iOS](https://github.com/newmaterialco/chowder-iOS) — Native iOS client for AI agent interaction with real-time thinking and tool activity display
- [notchi](https://github.com/sk-ruban/notchi) — macOS notch companion that reacts to Claude Code activity in real-time

## License

[MIT](LICENSE) © Po Kai Lee
