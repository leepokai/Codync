import { useState } from 'react'
import { isGroup, type Bot, type Entry } from '@shared/models'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { Spinner } from '../../components/Controls'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'

/** Picks the chat a message is sent on to ("Send to a channel"). */
export function ForwardSheet({ entry, close }: { entry: Entry; close: () => void }) {
  const store = useStore()
  const tr = useTr()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const bots = [...store.bots.values()].sort((a, b) => a.name.localeCompare(b.name))
  const members = (bot: Bot) => bot.members.map((id) => store.bots.get(id)).filter((b): b is Bot => !!b)
  const send = async (bot: Bot) => {
    setBusy(bot.id)
    setError(null)
    try {
      await store.forward(entry.id, bot.id)
      close()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setBusy(null)
    }
  }
  return (
    <SheetPage>
      <ModalHeader
        icon="arrowshape.turn.up.right"
        title={tr('Send to a channel')}
        subtitle={tr('The message arrives there as a forwarded copy.')}
        onClose={close}
      />
      <div className="thread-hairline" />
      <div className="group-scroll">
        <div className="group-form">
          {bots.map((bot) => (
            <button
              key={bot.id}
              className="member-row press"
              disabled={busy !== null}
              onClick={() => void send(bot)}
            >
              {isGroup(bot) ? <GroupAvatar members={members(bot)} size={32} /> : <BotAvatar bot={bot} size={32} animated={false} />}
              <div className="member-name">
                <Text truncate>{bot.name}</Text>
                {isGroup(bot) ? <Text size="caption" tone="tertiary">{tr('Group chat')}</Text> : null}
              </div>
              {busy === bot.id ? <Spinner size={14} /> : null}
            </button>
          ))}
          {bots.length === 0 ? <Text tone="secondary">{tr('No chats yet.')}</Text> : null}
          {error ? <Text role="alert" tone="danger">{error}</Text> : null}
        </div>
      </div>
    </SheetPage>
  )
}
