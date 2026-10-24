import type { ReactNode } from 'react'
import { useTr } from '../../lib/use-language'
import { Spacer } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { BotStore } from '../../store/bot-store'
import './settings.css'

export const isMac = window.codync.platform === 'darwin'
/** "this Mac" on macOS; "this computer" elsewhere. */
export const thisMac = isMac ? 'this Mac' : 'this computer'

/** The icon for a device by its account platform: phones, Macs, and other desktops. */
export function deviceIcon(platform: string | null | undefined) {
  return platform === 'ios' ? 'iphone' : platform === 'macos' ? 'laptopcomputer' : 'desktopcomputer'
}

/** A section's title row (the CardSection heading). */
export function SectionHeader({ title, children }: { title: string; children?: ReactNode }) {
  const tr = useTr()
  return (
    <div className="settings-section-header">
      <Text as="h3" size="body" weight="semibold" tone="text">{tr(title)}</Text>
      <Spacer />
      {children}
    </div>
  )
}

export const errorText = (e: unknown) => (e instanceof Error ? e.message : typeof e === 'object' && e && 'message' in e ? String(e.message) : String(e))

/** "3:27 PM", "Yesterday", "Monday" or "Sep 16" (kit's `RelativeTime.day`). */
export function relativeDay(date: Date, now = new Date()) {
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const days = Math.round((day(now) - day(date)) / 86_400_000)
  if (days === 0) return date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
  if (days === 1) return 'Yesterday'
  if (now.getTime() - date.getTime() < 6 * 86_400_000) return date.toLocaleDateString(undefined, { weekday: 'long' })
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

/** One short line for a computer's connection, used next to its name. */
export function statusText(store: BotStore) {
  const c = store.connection
  switch (c.kind) {
    case 'online':
      return store.mismatch ? 'Needs update' : 'Online'
    case 'connecting':
      return 'Connecting…'
    case 'computerOffline':
      return c.lastSeen ? `Offline · seen ${relativeDay(new Date(c.lastSeen))}` : 'Offline'
    case 'offline':
      return "Can't reach"
    case 'unauthorized':
      return c.message
    case 'unpaired':
      return 'Not paired'
  }
}

/** The live client, or null while the computer isn't connected (kit's `store.client`). */
export const liveClient = (store: BotStore) => (store.connection.kind === 'online' ? store.client : null)

/** "123 456" for a 6-digit code. */
export const spacedCode = (code: string) => (code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code)
