# iOS UI Architecture Vision (2026-03-21)

## Current State
- Single view: session list with flat navigation
- No tab system, no feature switching

## Future Features
- **Claude Code sessions** (current)
- **Codex** integration
- **Claude Remote Control / Cowork** mode
- **Overall Live Activity** vs **Individual Live Activity** mode switching

## Proposed Architecture

### Tab Bar with Liquid Glass

Bottom tab bar (or segmented control at top) with glass morph aesthetic:

```
┌──────────────────────────────────────┐
│  [Sessions]  [Codex]  [Remote]       │  ← liquid glass tabs
├──────────────────────────────────────┤
│                                      │
│  Session list / Codex view / etc.    │
│                                      │
└──────────────────────────────────────┘
```

Each tab = a different AI workflow:
- **Sessions**: Claude Code session monitoring (current functionality)
- **Codex**: Codex job tracking (future)
- **Remote**: Remote control / cowork mode (future)

### Live Activity Configuration (in Sessions tab)

Top of session list:
```
┌──────────────────────────────────────┐
│  [Overall]  [Individual]             │  ← mode switcher
├──────────────────────────────────────┤
│  Max sessions: [1] [2] [3] [4]       │  ← only in Overall mode
│  Primary: ByCrawl ★                  │  ← tap to set primary
├──────────────────────────────────────┤
│  ✦ ByCrawl    Opus    working        │
│  ● Codync     Opus    idle           │
│  ...                                 │
└──────────────────────────────────────┘
```

### Overall Live Activity Layout

```
Lock Screen (max 4 sessions):
┌──────────────────────────────────────┐
│  Codync                       $2.30  │
│                                      │
│  ★ ByCrawl    Opus    ⌨️ Running..   │  ← primary (highlighted color)
│  ● Codync     Opus    idle           │  ← normal row
│  ● SignalWeb  Sonnet  idle           │  ← normal row
│                                      │
│  ⌨️ Running command           0:05   │  ← primary's current tool
└──────────────────────────────────────┘

Dynamic Island: shows primary session only
Apple Watch: shows primary session only
```

### Design Principles

1. **Liquid glass tab bar**: frosted glass background, smooth transitions between tabs
2. **Progressive disclosure**: start with Sessions tab, add tabs as features are built
3. **Primary session**: single source of truth for Dynamic Island / Watch / notification focus
4. **Overall mode**: compact multi-session view, primary highlighted
5. **Individual mode**: current per-session Live Activity approach

### Implementation Priority

1. Live Activity redesign (Overall + Individual modes, primary session)
2. Top-level mode switcher UI
3. Tab bar skeleton (Sessions only, prepared for future tabs)
4. Codex tab (when ready)
5. Remote tab (when ready)

## Questions
- Tab bar style: bottom TabView or top segmented control?
- Should the liquid glass tab have animation between tabs (slide/morph)?
- Codex integration: what data to show? Job status, logs, cost?
