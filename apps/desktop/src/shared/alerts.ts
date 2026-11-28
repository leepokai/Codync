// A host's `alert` event (host/src/remote/push.rs): a bot is done, needs you or failed. The same text a phone's push
// shows; the desktop app shows it as a system notification when its user turned that on (docs/design/push-and-live-activity.md).

export type AlertKind = 'done' | 'needsInput' | 'failed'

export interface HostAlert {
  botId: string
  kind: AlertKind
  title: string
  body: string
  /** A workflow step's alert: clicking it opens this workflow instead of the bot's chat. */
  workflowId?: string
}

/** What the chat window asks the main process to show: `ref` is the bot's `computerId/botId`. */
export interface DesktopAlert {
  ref: string
  kind: AlertKind
  title: string
  body: string
  workflowId?: string
}

const KINDS: readonly AlertKind[] = ['done', 'needsInput', 'failed']

/** The event's `alert`, or null when it isn't one this app knows. */
export function parseAlert(raw: unknown): HostAlert | null {
  const a = raw as Partial<HostAlert> | null | undefined
  if (!a || typeof a.botId !== 'string' || typeof a.title !== 'string' || !KINDS.includes(a.kind as AlertKind)) return null
  const alert: HostAlert = { botId: a.botId, kind: a.kind as AlertKind, title: a.title, body: typeof a.body === 'string' ? a.body : '' }
  if (typeof a.workflowId === 'string') alert.workflowId = a.workflowId
  return alert
}

/** An alert kind as analytics names it (`desktop_notification_opened`). */
export type AlertAnalyticsKind = 'done' | 'needs_input' | 'failed'

export const alertAnalyticsKind = (kind: AlertKind): AlertAnalyticsKind => (kind === 'needsInput' ? 'needs_input' : kind)
