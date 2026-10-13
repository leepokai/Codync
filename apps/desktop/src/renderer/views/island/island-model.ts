import {
  FINISHED_MS, focusOf, isActive, islandFrame, islandMode, islandRect, othersOf, transitions,
  type IslandBot, type IslandNotice, type IslandPermission, type IslandScreen, type IslandSummary, type IslandTab,
} from '@shared/island'
import { Observable } from '../../lib/observable'

/** The bare notch peeks at once and opens after a rest; the compact island opens sooner. */
const OPEN_FROM_PEEK_MS = 650
const OPEN_FROM_COMPACT_MS = 200
const UNPEEK_MS = 600
/** Opened by hover, it folds soon after the pointer leaves; opened by a click, after a countdown. */
const LEAVE_MS = 800
export const PINNED_LEAVE_MS = 10_000

/** The island window's state: the chat window's summary, the pointer, the header's tab and what was put off. */
export class IslandModel extends Observable {
  summary: IslandSummary = { bots: [], permissions: [], usage: [] }
  screen: IslandScreen = { notch: null }
  open = false
  tab: IslandTab = 'auto'
  notice: IslandNotice | null = null
  /** Counting down to fold after the pointer left a pinned island. */
  closing = false
  /** The bot the message field writes to. */
  messageRef: string | null = null
  /** The shape grew on its last change: open with a spring, close without one. */
  growing = true
  private peek = false
  private pinned = false
  private hovering = false
  private deferred = new Set<string>()
  private statuses: Map<string, string> | null = null
  private hoverTimer: ReturnType<typeof setTimeout> | undefined
  private noticeTimer: ReturnType<typeof setTimeout> | undefined
  private shape = { width: 0, height: 0 }
  private keys = false
  /** The window took the keyboard for the message field; it keeps it until the island closes. */
  private keyboard = false

  start() {
    const island = window.codync.island
    island.onSummary((s) => this.apply(s))
    island.onScreen((s) => this.update(() => (this.screen = s)))
    island.onHover((inside) => this.hover(inside))
    island.onKey((answer) => this.answerByKey(answer))
    window.addEventListener('blur', () => {
      if (this.tab === 'message') this.close()
    })
    // Linux shapes the window to the island (main/island.ts): the pointer entering the window is the hover.
    if (window.codync.platform === 'linux') {
      document.documentElement.addEventListener('mouseenter', () => this.hover(true))
      document.documentElement.addEventListener('mouseleave', () => this.hover(false))
    }
    void island.state().then(({ screen, summary }) => {
      this.screen = screen
      this.apply(summary)
    })
  }

  get mode() {
    return islandMode(this.summary, { open: this.open, peek: this.peek, tab: this.tab, deferred: this.deferred, notice: this.notice })
  }

  get frame() {
    return islandFrame(this.mode, this.screen)
  }

  /** The approval to answer now: the first not put off, else the first. */
  get permission(): IslandPermission | null {
    const cards = this.summary.permissions
    return cards.find((p) => !this.deferred.has(p.entryId)) ?? cards[0] ?? null
  }

  get focus() {
    return focusOf(this.summary.bots)
  }

  /** The bots beside the big slot: pills (overview), the mini column and the compact cluster. */
  others(limit: number) {
    return othersOf(this.summary.bots, this.focus, limit)
  }

  get activeCount() {
    return this.summary.bots.filter(isActive).length
  }

  bot(ref: string | null) {
    return this.summary.bots.find((b) => b.ref === ref) ?? null
  }

  get noticeBot(): IslandBot | null {
    return this.bot(this.notice?.ref ?? null)
  }

  get messageBot(): IslandBot | null {
    return this.bot(this.messageRef) ?? this.focus
  }

  /** A click on the closed island opens it and keeps it open. */
  openNow() {
    this.update(() => {
      this.open = true
      this.pinned = true
      this.peek = false
    })
    this.foldWhenAway()
  }

  /** The header's tabs. Bots puts the approvals on screen off until another arrives. */
  setTab(tab: IslandTab) {
    this.update(() => {
      if (tab === 'bots') for (const p of this.summary.permissions) this.deferred.add(p.entryId)
      if (tab === 'message' && !this.messageRef) this.messageRef = this.focus?.ref ?? null
      this.tab = tab
      this.open = true
      this.pinned = true
    })
    // Only the message field takes the keyboard. It goes back when the island closes, not on the
    // next tab: giving it back blinks the window on macOS.
    if (tab === 'message') {
      this.keyboard = true
      window.codync.island.setFocusable(true)
    }
    this.foldWhenAway()
  }

  /** The message field's bot chip steps through the bots. */
  nextMessageBot() {
    const bots = this.summary.bots
    if (!bots.length) return
    const at = bots.findIndex((b) => b.ref === this.messageBot?.ref)
    this.update(() => (this.messageRef = bots[(at + 1) % bots.length]!.ref))
  }

  send(text: string) {
    const bot = this.messageBot
    if (!bot || !text.trim()) return
    window.codync.island.send(bot.ref, text)
    this.close()
  }

  respond(card: IslandPermission, optionId: string) {
    window.codync.island.respond(card.ref, card.entryId, optionId)
    if (this.open) this.close()
  }

  openBot(ref: string) {
    window.codync.island.openBot(ref)
    this.dismissNotice()
    this.close()
  }

  newBot() {
    window.codync.island.newBot()
    this.close()
  }

  openApp() {
    window.codync.island.openApp()
    this.close()
  }

  dismissNotice() {
    clearTimeout(this.noticeTimer)
    this.update(() => (this.notice = null))
  }

  close() {
    clearTimeout(this.hoverTimer)
    this.update(() => {
      this.open = false
      this.pinned = false
      this.peek = false
      this.closing = false
      this.tab = 'auto'
    })
    if (this.keyboard) window.codync.island.setFocusable(false)
    this.keyboard = false
  }

  private answerByKey(answer: 'allow' | 'deny') {
    const card = this.mode === 'approval' ? this.permission : null
    const option = card?.options.find((o) => o.kind === (answer === 'allow' ? 'allow_once' : 'reject_once'))
    if (card && option && card.answering === null) this.respond(card, option.id)
  }

  private apply(summary: IslandSummary) {
    const changes = this.statuses ? transitions(this.statuses, summary.bots) : []
    this.statuses = new Map(summary.bots.map((b) => [b.ref, b.status]))
    this.update(() => {
      this.summary = summary
      // Answered and expired cards leave the put-off set.
      for (const id of this.deferred) if (!summary.permissions.some((p) => p.entryId === id)) this.deferred.delete(id)
      // A failed bot that works again no longer needs the notice.
      const shown = this.notice ? summary.bots.find((b) => b.ref === this.notice?.ref) : undefined
      if (shown && isActive(shown)) this.notice = null
      const next = changes.at(-1)
      if (next) this.showNotice(next)
    })
  }

  /** Done fades after a moment; failed stays until dismissed. */
  private showNotice(notice: IslandNotice) {
    clearTimeout(this.noticeTimer)
    this.notice = notice
    if (notice.kind === 'finished') this.noticeTimer = setTimeout(() => this.update(() => (this.notice = null)), FINISHED_MS)
  }

  private hover(inside: boolean) {
    this.hovering = inside
    clearTimeout(this.hoverTimer)
    const mode = this.mode
    if (inside) {
      if (this.closing) this.update(() => (this.closing = false))
      if (mode === 'hidden') {
        this.update(() => (this.peek = true))
        this.hoverTimer = setTimeout(() => this.openByHover(), OPEN_FROM_PEEK_MS)
      } else if (mode === 'peek' || mode === 'compact') {
        this.hoverTimer = setTimeout(() => this.openByHover(), OPEN_FROM_COMPACT_MS)
      }
      return
    }
    if (this.open) {
      if (this.pinned) this.foldWhenAway()
      else this.hoverTimer = setTimeout(() => this.close(), LEAVE_MS)
    } else if (this.peek) {
      this.hoverTimer = setTimeout(() => this.update(() => (this.peek = false)), UNPEEK_MS)
    }
  }

  /**
   * Kept open with the pointer elsewhere (it left, or a click came through accessibility): fold
   * after a countdown. The message field stays until it sends, Esc or a click in another app.
   */
  private foldWhenAway() {
    clearTimeout(this.hoverTimer)
    const counting = this.open && !this.hovering && this.tab !== 'message'
    if (counting !== this.closing) this.update(() => (this.closing = counting))
    if (counting) this.hoverTimer = setTimeout(() => this.close(), PINNED_LEAVE_MS)
  }

  private openByHover() {
    if (!this.hovering) return
    this.update(() => {
      this.open = true
      this.peek = false
    })
  }

  private update(change: () => void) {
    change()
    const frame = this.frame
    if (frame.width !== this.shape.width || frame.height !== this.shape.height) {
      this.growing = frame.width * frame.height >= this.shape.width * this.shape.height
      this.shape = { width: frame.width, height: frame.height }
      window.codync.island.setShape(islandRect(frame, window.innerWidth))
    }
    const keys = this.mode === 'approval'
    if (keys !== this.keys) {
      this.keys = keys
      window.codync.island.setKeys(keys)
    }
    this.changed()
  }
}
