import { callConversation, withCallEntries, type CallMessage } from '@shared/calls'
import type { Entry } from '@shared/models'
import { useEffect, useRef } from 'react'
import { BotOrGroupAvatar } from '../../components/Avatar'
import { Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { copyText } from '../../lib/image-clipboard'
import { useFetched } from '../../lib/use-fetched'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { Bubble } from '../thread/ChatRows'
import { callLength, relativeTime } from '../thread/chat-items'
import { MarkdownText } from '../thread/Markdown'
import '../thread/bot-conversation.css'
import './call.css'

/** A voice call's line in the chat: its title (written once the call ends) and length; it opens the call. */
export function CallLineRow({ entry, open }: { entry: Entry; open: () => void }) {
  const tr = useTr()
  const title = tr(entry.data.text || 'Voice chat')
  return (
    <button className="press call-line-row" onClick={open} title={tr('Open the call')} aria-label={`${title}${callLength(entry.data.callSeconds)}`}>
      <Icon name="waveform" size={10} color="var(--secondary)" />
      <Text size="footnote" tone="text" truncate>{title}</Text>
      <Text size="footnote" tone="secondary" tabular>{callLength(entry.data.callSeconds)}</Text>
      <Icon name="chevron.right" size={10} color="var(--tertiary)" />
    </button>
  )
}

/**
 * What a voice call said, oldest first: the user's and the voice's words, and the messages it carried to the bot
 * and back, which the chat itself leaves out.
 */
export function CallConversationView({ botId, lineId }: { botId: string; lineId: string }) {
  const tr = useTr()
  const store = useStore()
  const live = store.allEntries(botId)
  const line = live.find((e) => e.id === lineId)
  const call = line?.data.call
  // The call's own messages, which the loaded chat may not reach back to; null while they load.
  const { fetched, error } = useFetched(() => (call ? store.callEntries(botId, call) : null), [store, botId, call])
  const messages = line ? callConversation(withCallEntries(live, fetched ?? []), line) : []
  const scroller = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages.length])
  return (
    <div className="bot-conversation">
      <ModalHeader title={tr(line?.data.text || 'Voice chat')} />
      <div className="bot-conversation-scroll" ref={scroller}>
        <VStack className="bot-conversation-rows">
          {line ? <CallHeader botId={botId} line={line} /> : null}
          {error ? <Text as="div" size="footnote" tone="danger" className="call-note">{error}</Text> : null}
          {fetched === null && !error && call ? <div className="call-note" aria-label={tr('Loading the call')}><Spinner /></div> : null}
          {!error && fetched !== null && messages.length === 0 ? <Text as="div" size="footnote" tone="tertiary" className="call-note">{tr('Nothing was said on this call.')}</Text> : null}
          {messages.map((m, i) => (
            <div key={i} className={i === 0 ? undefined : opensExchange(m, messages[i - 1]) ? 'call-gap-wide' : 'call-gap'}>
              <CallMessageRow message={m} />
            </div>
          ))}
        </VStack>
      </div>
    </div>
  )
}

/** Who the call was with, when it ended and how long it lasted, above what was said. */
function CallHeader({ botId, line }: { botId: string; line: Entry }) {
  const tr = useTr()
  const store = useStore()
  const bot = store.bots.get(botId)
  return (
    <VStack gap={4} align="center" className="call-header">
      {bot ? (
        <>
          <div className="call-header-avatar"><BotOrGroupAvatar bot={bot} members={store.members(bot)} size={52} animated={false} /></div>
          <Text size="headline" tone="text">{bot.name}</Text>
        </>
      ) : null}
      <Text size="footnote" tone="tertiary" tabular>{relativeTime.separator(line.createdAt, tr)}{callLength(line.data.callSeconds)}</Text>
    </VStack>
  )
}

/** Whether `message` starts a new exchange (you speak after the other side), which gets more room above it. */
const opensExchange = (message: CallMessage, previous?: CallMessage) => message.speaker === 'user' && previous?.speaker !== 'user'

function CallMessageRow({ message }: { message: CallMessage }) {
  const copy = () => [{ title: 'Copy', icon: 'square.on.square', action: () => void copyText(message.text) }]
  switch (message.speaker) {
    case 'user':
      return (
        <div className="message user">
          <div className="message-body"><Bubble className="bubble user selectable" items={copy}><Text>{message.text}</Text></Bubble></div>
        </div>
      )
    case 'voice':
      return <Text as="div" tone="text" className="selectable">{message.text}</Text>
    case 'voiceToBot':
      return <Text as="div" size="footnote" tone="secondary" className="selectable">↳ {message.text}</Text>
    case 'bot':
      return (
        <div className="message agent">
          <div className="message-body"><Bubble className="bubble agent selectable" items={copy}><MarkdownText text={message.text} /></Bubble></div>
        </div>
      )
  }
}
