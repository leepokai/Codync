import { useRef, useState } from 'react'
import { IconButton } from '../../components/Controls'
import { AnchoredMenu, type MenuItem } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { usePref } from '../../lib/prefs'
import { useTr } from '../../lib/use-language'
import type { BotRef, RosterItem } from '../../store/app-model'
import { useStore } from '../../store/context'
import { ThreadChat } from '../chat/ThreadChat'
import { ThreadStart, type ThreadWith } from '../chat/ThreadStart'
import { threadBots } from '../ThreadsPage'
import { NO_BAR_SLOTS, WindowBarSlots } from '../WindowBar'
import { workflowChats, type ChatAt } from './pins'

/** The most chats a workflow keeps to go back to. */
const KEPT = 20

/**
 * Beside the canvas: the thread the workflow is built in by talking. A new chat starts from the page a thread starts
 * from, with no bot unless one is picked; what it sends opens a thread whose workflow tools reach this workflow only
 * (`setup.workflow`), in the workflow's own folder, and the canvas shows each save as it lands. Earlier chats stay a
 * click away (Previous chats), newest first.
 */
export function WorkflowChat({ workflowId, roster, openChat }: {
  workflowId: string
  /** The bots and the box of threads without a bot, on every computer: this one's are used. */
  roster: RosterItem[]
  openChat: (ref: BotRef) => void
}) {
  const tr = useTr()
  const store = useStore()
  const [all, setAll] = usePref(workflowChats(store.computer.id))
  const [shown, setShown] = useState<ChatAt | 'new' | null>(null)
  const [picked, setPicked] = useState<BotRef | null>(null)
  const [listing, setListing] = useState(false)
  const historyButton = useRef<HTMLButtonElement>(null)
  const mine = roster.filter((r) => r.ref.computerId === store.computer.id)
  // The chats whose threads this computer still has, newest first.
  const chats = (all[workflowId] ?? []).flatMap((at) => {
    const of = mine.find((r) => r.ref.botId === at.botId)
    const task = of?.bot.work.find((t) => t.id === at.taskId && t.thread)
    return of && task ? [{ at, of, task }] : []
  })
  const current = shown === 'new' ? undefined : shown ? chats.find((c) => c.at.taskId === shown.taskId) : chats[0]
  const started = (at: ChatAt) => {
    setAll({ ...all, [workflowId]: [at, ...(all[workflowId] ?? []).filter((c) => c.taskId !== at.taskId)].slice(0, KEPT) })
    setShown(at)
  }
  const previous = (): MenuItem[] => chats.map((c) => ({
    title: c.task.title, verbatim: true, selected: c === current, action: () => setShown(c.at),
  }))
  const bots = threadBots(mine)
  const plain: ThreadWith | undefined = store.startsThreadsWithoutBot ? { ref: { computerId: store.computer.id, botId: '' }, store, bot: null } : undefined
  const chosen = picked ? bots.find((r) => r.ref.botId === picked.botId) : undefined
  const first = chosen ?? plain ?? bots[0]
  return (
    <div className="workflow-chat">
      <HStack gap={4} className="workflow-chat-bar">
        <Text weight="semibold" tone="text" grow>{tr('Chat')}</Text>
        {chats.length ? <IconButton title="Previous chats" icon="clock" size={26} buttonRef={historyButton} menuOpen={listing} onClick={() => setListing((o) => !o)} /> : null}
        {current ? <IconButton title="New chat" icon="square.and.pencil" size={26} onClick={() => setShown('new')} /> : null}
        <AnchoredMenu open={listing} onClose={() => setListing(false)} anchor={historyButton} items={previous} />
      </HStack>
      {/* The thread's title and details stay in the panel: the window's bar is the canvas's. */}
      <WindowBarSlots.Provider value={NO_BAR_SLOTS}>
        <div className="workflow-chat-body">
          {current ? (
            <ThreadChat key={`${current.at.botId}/${current.task.id}`} bot={current.of.bot} task={current.task} openChat={() => openChat(current.of.ref)} />
          ) : first ? (
            <ThreadStart
              key={first.ref.botId || 'plain'}
              bots={bots}
              of={first}
              choose={(ref, none) => setPicked(none ? null : ref)}
              started={(ref, taskId) => started({ botId: ref.botId, taskId })}
              base={{ workflow: workflowId }}
              folder={tr('Workflow folder')}
              heading={tr('Build this workflow by talking')}
              placeholder={tr('What should this workflow do?')}
            />
          ) : null}
        </div>
      </WindowBarSlots.Provider>
    </div>
  )
}
