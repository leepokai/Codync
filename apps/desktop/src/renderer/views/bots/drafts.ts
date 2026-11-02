import { useEffect, useRef, useState } from 'react'
import { rebase } from '@shared/draft-rebase'
import { AUTO_MODE } from '@shared/approvals'
import { draftOf, type Bot, type BotDraft } from '@shared/models'
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

/** A fresh draft: a random look, Claude, a personal workspace, the agent's Auto mode asking about the rest. */
export function newDraft(): BotDraft {
  return { name: '', description: '', avatarColor: pick(AVATAR_COLORS).id, avatarShape: pick(AVATAR_SHAPES), backend: 'claude', cwd: '', permission: 'ask', mode: AUTO_MODE }
}

/** A saved bot keeps a name; only a new one may go without (the host names it after its first conversations). */
export const isValid = (d: BotDraft) => d.name.trim() !== ''

/** What the host should get: no stale command unless the agent is custom. */
export const normalized = (d: BotDraft): BotDraft => (d.backend === 'custom' ? d : { ...d, command: null })

/**
 * Picks an installed agent we know (a registry agent nobody chose is never the default);
 * empty cwd requests a personal workspace from the host. Connectors and skills stay unset:
 * the host turns on every one it can run.
 */
export function fillDefaults(store: BotStore, draft: BotDraft): BotDraft {
  const next = { ...draft }
  const backends = store.hello?.backends ?? []
  const first = backends.find((b) => b.available && b.curated === true)
  if (backends.find((b) => b.id === next.backend)?.available !== true && first) next.backend = first.id
  return next
}

/** The connectors and skills installed on the computer (kit `refreshPlugins`). */
async function loadPlugins(store: BotStore): Promise<Plugins> {
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

/**
 * Keeps an open settings form on the host's copy of its bot: a change made elsewhere (a bot that manages the team,
 * another device) replaces each field the user hasn't edited here, so saving the form never puts back the old value.
 */
export function useFollowSaved(bot: Bot | undefined, change: (update: (d: BotDraft) => BotDraft) => void) {
  const saved = bot ? JSON.stringify(draftOf(bot)) : null
  const last = useRef(saved)
  useEffect(() => {
    const old = last.current
    last.current = saved
    if (!old || !saved || old === saved) return
    const [base, fresh] = [JSON.parse(old) as BotDraft, JSON.parse(saved) as BotDraft]
    if (base.id === fresh.id) change((d) => rebase(d, base, fresh))
  }, [saved])
}

export const lastPathComponent = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path

export const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e))
