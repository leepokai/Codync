import { useRef, useState } from 'react'
import type { Bot } from '@shared/models'
import { IconButton } from '../../components/Controls'
import { Text } from '../../components/Text'
import { useHostTerminal } from '../../components/use-host-terminal'
import { useTr } from '../../lib/use-language'
import './terminal-tab.css'

/**
 * The bot's terminal: the user's own shell on the bot's computer, in its folder or `thread`'s (`botTerminal`). It keeps running while
 * the tab is away and shows again as it was; once it exits, a click opens a new one.
 */
export function TerminalTab({ bot, thread }: { bot: Bot; thread?: string | null }) {
  const tr = useTr()
  const surface = useRef<HTMLDivElement>(null)
  const [run, setRun] = useState(0)
  // A thread is a bot of its own on the host: its terminal opens in the thread's folder (a worktree, say).
  const at = thread ?? bot.id
  const { exitCode, error } = useHostTerminal(surface, `${at}/${run}`, {
    open: async (client, cols, rows) => (await client.call<{ term: string }>('botTerminal', { botId: at, cols, rows })).term,
    closeOnLeave: false,
  })
  return (
    <div className="terminal-tab">
      <div ref={surface} className="terminal-tab-surface" />
      {error || exitCode !== null ? (
        <div className="terminal-tab-ended">
          <Text size="caption" tone={error ? 'danger' : 'tertiary'} grow lines={2}>{error ?? (exitCode === 0 ? tr('Finished.') : tr('Ended with an error ({code}).').replaceAll('{code}', () => String(exitCode)))}</Text>
          <IconButton title="New terminal" icon="arrow.clockwise" size={24} onClick={() => setRun((n) => n + 1)} />
        </div>
      ) : null}
    </div>
  )
}
