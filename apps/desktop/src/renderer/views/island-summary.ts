import { useEffect } from 'react'
import type { IslandBot, IslandPermission, IslandSummary } from '@shared/island'
import { folderName, isGroup, isWorking, type Bot, type Entry } from '@shared/models'
import { useModels } from '../lib/observable'
import { refKey, type AppModel, type RosterItem } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import { botDetail } from './tray-summary'
import { optionLabel, permissionHeadline, sortedOptions } from './thread/PermissionCard'

/** Tool calls in the island's ticker. */
const STEPS = 3

let sent = ''
let pending: ReturnType<typeof setTimeout> | null = null

/**
 * Sends the notch island what it shows when the stores change: at most every 150 ms while
 * replies stream (reading the stores when it fires), and only when that changed.
 */
export function useIslandSummary(app: AppModel) {
  useModels([...app.stores.values()])
  useEffect(() => {
    if (pending) return
    pending = setTimeout(() => {
      pending = null
      const summary = islandSummary(app)
      const json = JSON.stringify(summary)
      if (json === sent) return
      sent = json
      window.codync.island.setSummary(summary)
    }, 150)
  })
}

function islandSummary(app: AppModel): IslandSummary {
  const many = app.computers.length > 1
  return {
    bots: app.roster
      .filter((i) => !isGroup(i.bot))
      .map((i) => islandBot(i.store, i.bot, refKey(i.ref), many ? `${i.bot.name} · ${i.computer.name}` : i.bot.name)),
    permissions: app.roster.flatMap(pendingCards),
    usage: app.local?.usage.providers ?? [],
  }
}

function islandBot(store: BotStore, bot: Bot, ref: string, name: string): IslandBot {
  const entries = store.allEntries(bot.id)
  return {
    ref,
    name,
    shape: bot.avatarShape,
    customIcon: bot.customIcon,
    color: bot.avatarColor,
    status: bot.status,
    backend: store.backendName(bot.backend),
    folder: folderName(bot),
    detail: botDetail(bot),
    steps: isWorking(bot)
      ? entries.filter((e) => e.kind === 'tool').slice(-STEPS).map((e) => ({ title: e.data.title || 'Use a tool', kind: e.data.toolKind ?? 'other' }))
      : [],
    error: bot.status === 'error' ? (lastError(entries) ?? null) : null,
  }
}

/** The text of the conversation's last error notice. */
const lastError = (entries: Entry[]) => entries.findLast((e) => e.kind === 'notice' && e.data.style === 'error')?.data.text

/** A conversation's unanswered approval cards; in a group, the member that asks. */
function pendingCards({ ref, bot, store }: RosterItem): IslandPermission[] {
  return store
    .allEntries(bot.id)
    .filter((e) => e.kind === 'permission' && e.data.status === 'pending')
    .map((e) => {
      const author = (e.data.author ? store.bots.get(e.data.author) : undefined) ?? bot
      return {
        ref: refKey(ref),
        entryId: e.id,
        bot: { customIcon: author.customIcon, name: author.name, shape: author.avatarShape, color: author.avatarColor, backend: store.backendName(author.backend) },
        headline: permissionHeadline(e.data.toolKind),
        title: e.data.title ?? '',
        options: sortedOptions(e.data.options ?? []).map((o) => ({ id: o.optionId, label: optionLabel(o), kind: o.kind })),
        answering: store.answering.get(e.id) ?? null,
      }
    })
}
