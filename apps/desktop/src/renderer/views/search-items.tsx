import { isGroup } from '@shared/models'
import { AvatarWithStatus, BotAvatar, NoBotAvatar } from '../components/Avatar'
import { Icon } from '../components/Icon'
import { refKey, type BotRef, type RosterItem } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import type { ThreadPick } from './ThreadsPage'
import type { PaletteItem } from './palette-items'
import { canOpenWorkflows } from './workflows/pins'

// What each list's search finds: the rows of that list, matched by what they show.

/** The bots, by name, description or last message; a group's subtitle lists its members. */
export function botItems(roster: RosterItem[], open: (ref: BotRef) => void): PaletteItem[] {
  // A paused bot's avatar carries a labelled mark (AvatarWithStatus), so a reader hears Paused here too.
  return roster.map((item) => {
    const members = isGroup(item.bot) ? item.store.members(item.bot) : []
    const names = members.map((m) => m.name).join(', ')
    return {
      key: refKey(item.ref),
      title: item.bot.name,
      subtitle: item.bot.description || (names === item.bot.name ? '' : names),
      leading: <AvatarWithStatus bot={item.bot} members={members} size={32} />,
      words: [item.bot.name, item.bot.description, item.bot.lastMessage ?? ''],
      pick: () => open(item.ref),
    }
  })
}

/** The threads, newest first, by title or their bot's name. */
export function threadItems(roster: RosterItem[], open: (thread: ThreadPick) => void): PaletteItem[] {
  return roster
    .flatMap((of) => of.bot.work.filter((task) => task.thread).map((task) => ({ of, task })))
    .sort((a, b) => b.task.createdAt - a.task.createdAt)
    .map(({ of, task }) => ({
      key: `${refKey(of.ref)}/${task.id}`,
      title: task.title,
      subtitle: of.bot.plain ? of.store.hostName : of.bot.name,
      leading: of.bot.plain ? <NoBotAvatar size={32} /> : <BotAvatar bot={of.bot} size={32} animated={false} />,
      words: [task.title, of.bot.plain ? '' : of.bot.name],
      pick: () => open({ ref: of.ref, taskId: task.id }),
    }))
}

/** The workflows of the computers that offer them, by title or computer. */
export function workflowItems(stores: BotStore[], open: (computerId: string, workflowId: string) => void): PaletteItem[] {
  return stores.filter(canOpenWorkflows).flatMap((store) => store.workflows.map((workflow) => ({
    key: `${store.computer.id}/${workflow.id}`,
    title: workflow.title,
    subtitle: store.hostName,
    leading: <Icon name="flowchart" size={20} color="var(--secondary)" />,
    words: [workflow.title, store.hostName],
    pick: () => open(store.computer.id, workflow.id),
  })))
}
