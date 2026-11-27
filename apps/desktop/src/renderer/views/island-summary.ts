import { useEffect } from 'react'
import { islandLastError, islandReply, islandTurnSteps, type IslandBot, type IslandPermission, type IslandSummary } from '@shared/island'
import { folderName, isGroup, isWorking, type Bot } from '@shared/models'
import { outsideStatus } from '@shared/work'
import { messagePreview } from '../lib/message-preview'
import { useModels } from '../lib/observable'
import { parseRef, refKey, type AppModel, type RosterItem } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import { botDetail } from './tray-summary'
import { optionLabel, permissionHeadline, sortedOptions } from './chat/PermissionCard'
import { trNow } from '../lib/l10n.ts'

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
      window.codync.island.setSummary(summary)
      sent = json
      for (const { ref, completion } of summary.bots) {
        if (!completion) continue
        const bot = parseRef(ref)
        if (bot) app.store(bot.computerId)?.consumeCompletion(bot.botId, completion.id)
      }
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
  const away = isWorking(bot) && Boolean(bot.workingChat && bot.workingChat !== bot.id)
  const workingEntries = away && bot.workingChat ? store.allEntries(bot.workingChat) : entries
  const lastErr = islandLastError(entries)
  const completionId = store.completionFor(bot)
  return {
    ref,
    name,
    shape: bot.avatarShape,
    customIcon: bot.customIcon,
    color: bot.avatarColor,
    status: outsideStatus(bot),
    chat: bot.status,
    away,
    completion: typeof completionId === 'string' ? islandReply(entries, messagePreview, completionId) : completionId,
    reply: bot.completionReplyId === undefined ? islandReply(entries, messagePreview) : undefined,
    legacyStreamRev: bot.completionReplyId === undefined ? store.replayedThrough : undefined,
    backend: store.backendName(bot.backend),
    folder: folderName(bot, trNow),
    detail: botDetail(bot),
    steps: islandTurnSteps(bot, entries, workingEntries, STEPS, trNow('Use a tool')),
    error: bot.status === 'error' ? (lastErr?.text || null) : null,
    errorId: lastErr?.id ?? null,
  }
}

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
