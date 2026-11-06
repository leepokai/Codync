import { callConversation, withCallEntries, type CallMessage } from '@shared/calls'
import type { Entry } from '@shared/models'
import { useEffect, useRef } from 'react'
import { Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useFetched } from '../../lib/use-fetched'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { Bubble } from '../thread/ChatRows'
import { MarkdownText } from '../thread/Markdown'
import '../thread/bot-conversation.css'

const length = (seconds?: number) =>
  seconds === undefined ? '' : ` · ${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`

/** A voice call's line in the chat: its title (written once the call ends) and length; it opens the call. */
export function CallLineRow({ entry, open }: { entry: Entry; open: () => void }) {
  const tr = useTr()
  const title = tr(entry.data.text || 'Voice chat')
  return (
    <button className="press call-line-row" onClick={open} title={tr('Open the call')} aria-label={`${title}${length(entry.data.callSeconds)}`}>
      <Icon name="waveform" size={10} color="var(--secondary)" />
      <Text size="footnote" tone="text" truncate>{title}</Text>
      <Text size="footnote" tone="secondary" tabular>{length(entry.data.callSeconds)}</Text>
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
        <VStack gap={10} className="bot-conversation-rows">
          {error ? <Text as="div" size="footnote" tone="danger" className="bot-conversation-note">{error}</Text> : null}
          {fetched === null && !error && call ? <div className="bot-conversation-note" aria-label={tr('Loading the call')}><Spinner /></div> : null}
          {!error && fetched !== null && messages.length === 0 ? <Text as="div" size="footnote" tone="tertiary" className="bot-conversation-note">{tr('Nothing was said on this call.')}</Text> : null}
          {messages.map((m, i) => <CallMessageRow key={i} message={m} />)}
        </VStack>
      </div>
    </div>
  )
}

function CallMessageRow({ message }: { message: CallMessage }) {
  const copy = () => [{ title: 'Copy', icon: 'square.on.square', action: () => window.codync.app.copy(message.text) }]
  switch (message.speaker) {
    case 'user':
      return (
        <div className="message user">
          <div className="message-body"><Bubble className="bubble user selectable" items={copy}><Text>{message.text}</Text></Bubble></div>
        </div>
      )
    case 'voice':
      return <Text as="div" tone="secondary" className="selectable">{message.text}</Text>
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
