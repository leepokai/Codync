import type { Bot } from '../../shared/models.ts'
import { canCutIn, canRetry, isLive, wherePath, workSections, type WorkTask } from '../../shared/work.ts'
import type { MenuItem } from '../components/Menu'

/** What a thread's menu does: the page and the bot's box do the work, the menu only offers it. */
export interface ThreadMenuActions {
  /** Shows the thread beside the list. */
  open: () => void
  /** Opens the bot's chat, at the card for one that needs the user. */
  openChat: () => void
  /** Starts another thread with the same bot. */
  startAnother: () => void
  /** Asks before stopping it (`StopWorkDialog`). */
  stop: () => void
  /** Asks which task gives way (`CutInDialog`). */
  cutIn: () => void
  first: () => void
  retry: () => void
  drop: () => void
  copy: (text: string) => void
}

/**
 * A thread's menu on the Threads page, from its `…` button or a right-click: where it goes (the thread, its bot's chat,
 * another thread with the bot), what its bot's box does with it (the actions its bot's Details offers on the row: Stop
 * while it runs; First, Cut in and Drop while it waits; Try again and Drop once it ended), then what can be copied.
 * `canStart`: the bot can start another thread now. Only what applies is listed.
 */
export function threadMenuItems(bot: Bot, task: WorkTask, canStart: boolean, act: ThreadMenuActions): MenuItem[] {
  const { box } = workSections(bot.work, bot.maxThreads)
  const at = box.findIndex((t) => t.id === task.id)
  const path = wherePath(task)
  const go: MenuItem[] = [
    ...(task.thread ? [{ title: 'Open thread', icon: 'arrow.triangle.branch', action: act.open }] : []),
    // The box of threads without a bot has no chat of its own.
    ...(bot.plain ? [] : [{ title: task.status === 'needsYou' ? 'Answer in chat' : 'Open chat', icon: 'bubble.left', action: act.openChat }]),
    ...(canStart ? [{ title: 'New thread', icon: 'square.and.pencil', action: act.startAnother }] : []),
  ]
  const inBox: MenuItem[] = [
    ...(isLive(task) ? [{ title: 'Stop', icon: 'stop.circle', action: act.stop }] : []),
    ...(at > 0 ? [{ title: 'First', icon: 'arrow.up.to.line', action: act.first }] : []),
    ...(at === 0 && canCutIn(bot.work, bot.maxThreads) ? [{ title: 'Cut in', icon: 'arrow.turn.down.right', action: act.cutIn }] : []),
    ...(canRetry(task) ? [{ title: 'Try again', icon: 'arrow.clockwise', action: act.retry }] : []),
  ]
  const copy: MenuItem[] = [
    { title: 'Copy title', icon: 'doc.on.doc', action: () => act.copy(task.title) },
    ...(path ? [{ title: 'Copy folder path', icon: 'folder', action: () => act.copy(path) }] : []),
  ]
  // Drop takes it out of the box for good: last, apart, in red.
  const drop: MenuItem[] = isLive(task) ? [] : [{ title: 'Drop', icon: 'xmark', destructive: true, action: act.drop }]
  return [go, inBox, copy, drop]
    .filter((part) => part.length > 0)
    .flatMap((part, i) => (i === 0 ? part : [{ ...part[0]!, divider: true }, ...part.slice(1)]))
}
