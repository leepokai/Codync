import { useEffect, useState } from 'react'
import type { Bot, BotDraft } from '@shared/models'
import { AVATAR_COLORS, AVATAR_SHAPES } from '../../lib/theme'
import type { BotStore } from '../../store/bot-store'

/** An installed connector (host `connectors`). */
export interface InstalledConnector {
  id: string
  name: string
  description: string
  kind: string
  command?: string | null
  url?: string | null
}

/** An installed skill (host `skills`). */
export interface InstalledSkill {
  id: string
  name: string
  description: string
}

export interface Plugins {
  connectors: InstalledConnector[]
  skills: InstalledSkill[]
}

const pick = <T>(list: T[]) => list[Math.floor(Math.random() * list.length)]!

/** A fresh draft: a random look, Claude, a personal workspace, automatic approvals. */
export function newDraft(): BotDraft {
  return { name: '', description: '', avatarColor: pick(AVATAR_COLORS).id, avatarShape: pick(AVATAR_SHAPES), backend: 'claude', cwd: '', permission: 'auto' }
}

/** A new bot may go without a name: the host names it after its first conversations. */
export const isValid = (d: BotDraft) => !d.id ||d.name.trim() !== ''

/** What the host should get: no stale command unless the agent is custom. */
export const normalized = (d: BotDraft): BotDraft => (d.backend === 'custom' ? d : { ...d, command: null })

/**
 * Picks an installed agent we know (a registry agent nobody chose is never the default);
 * empty cwd requests a personal workspace from the host.
 */
export function fillDefaults(store: BotStore, draft: BotDraft, plugins: Plugins): BotDraft {
  const next = { ...draft }
  const backends = store.hello?.backends ?? []
  const first = backends.find((b) => b.available && b.curated === true)
  if (backends.find((b) => b.id === next.backend)?.available !== true && first) next.backend = first.id
  // Connectors and skills start on; left unset, the host turns on every one.
  if (next.connectors === undefined && plugins.connectors.length) next.connectors = plugins.connectors.map((c) => c.id)
  if (next.skills === undefined && plugins.skills.length) next.skills = plugins.skills.map((s) => s.id)
  return next
}

/** The connectors and skills installed on the computer (kit `refreshPlugins`). */
export async function loadPlugins(store: BotStore): Promise<Plugins> {
  const client = store.client
  if (!client) return { connectors: [], skills: [] }
  const [c, s] = await Promise.allSettled([
    client.call<{ items: InstalledConnector[] }>('connectors'),
    client.call<{ items: InstalledSkill[] }>('skills'),
  ])
  return { connectors: c.status === 'fulfilled' ? c.value.items : [], skills: s.status === 'fulfilled' ? s.value.items : [] }
}

export function usePlugins(store: BotStore): Plugins | null {
  const [plugins, setPlugins] = useState<Plugins | null>(null)
  const online = store.connection.kind === 'online'
  useEffect(() => {
    let live = true
    void loadPlugins(store).then((p) => live && setPlugins(p))
    return () => {
      live = false
    }
  }, [store, online])
  return plugins
}

/** Creates an unnamed bot with sensible defaults and opens it (desktop compose flow). */
export async function createDefaultBot(store: BotStore): Promise<Bot> {
  const bot = await store.save(fillDefaults(store, newDraft(), await loadPlugins(store)))
  store.setSelection(bot.id)
  return bot
}

export const lastPathComponent = (path: string) => path.split('/').filter(Boolean).pop() ?? path

export const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e))
