// Pinned workflows stay on this device. The pin key matches the phone: pinnedWorkflows.{computerId}.

import type { BotStore } from '../../store/bot-store'
import { pref, type Pref } from '../../lib/prefs'

const MAX_PINS = 20
const cache = new Map<string, Pref<string[]>>()

export function workflowPins(computerId: string) {
  let stored = cache.get(computerId)
  if (!stored) {
    stored = pref<string[]>(`pinnedWorkflows.${computerId}`, [])
    cache.set(computerId, stored)
  }
  return stored
}

/** Where a chat's thread is: its box (a bot, or the box of threads without one) and its task. */
export interface ChatAt { botId: string; taskId: string }

const builderCache = new Map<string, Pref<Record<string, ChatAt[]>>>()

/** The threads each workflow was built in (WorkflowChat), by workflow id, newest first. */
export function workflowChats(computerId: string) {
  let stored = builderCache.get(computerId)
  if (!stored) {
    stored = pref<Record<string, ChatAt[]>>(`workflowChats.${computerId}`, {})
    builderCache.set(computerId, stored)
  }
  return stored
}

export function pinWorkflow(computerId: string, id: string, pinned: boolean) {
  const stored = workflowPins(computerId)
  const without = stored.get().filter((item) => item !== id)
  stored.set(pinned ? [...without, id].slice(-MAX_PINS) : without)
}

export function isWorkflowPinned(computerId: string, id: string) {
  return workflowPins(computerId).get().includes(id)
}

/** Same reachability rule as Extensions: online, and running a Codync this app can talk to. */
export const reachable = (store: BotStore) => store.shownConnection.kind === 'online' && !store.mismatch

/** Reachable, and a host that says it has workflows. */
export function canOpenWorkflows(store: BotStore) {
  return reachable(store) && store.hello?.workflows === true && !store.workflowMethodMissing
}
