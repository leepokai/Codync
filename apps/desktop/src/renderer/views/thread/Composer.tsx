import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react'
import { groupFastMode } from '@shared/fast-mode'
import { artifactDraftKey, artifactQuote, type Artifact } from '@shared/artifacts'
import { isGroup, isWorkingIn, quoteOf, type Bot, type Entry } from '@shared/models'
import { BotAvatar } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { IconButton } from '../../components/Controls'
import { RecordingPill } from '../../components/RecordingPill'
import { usePresence } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { MAX_FILE_SIZE, type OutgoingFile } from '../../store/bot-store'
import { pluginsFor } from '../marketplace/plugins'
import { composerEnter } from './composer-keyboard'
import { CommandMenu, useCommandMenu } from './CommandMenu'
import { ComposerAttachment } from './ComposerAttachment'
import { QuoteRow } from './ChatRows'
import { useDictation } from './dictation-platform'
import { DictationEngineMenu } from './DictationEngineMenu'
import { ContextUsageIndicator } from './ContextUsageIndicator'
import { builtInCommand, slashCommands, slashQuery, type SlashCommand } from './slash-commands'
import { useVoiceShortcut } from './voice-shortcut-keys'

type Trailing = 'interrupt' | 'stop' | 'call' | 'send'

/**
 * The message box under a chat: send, or stop while the bot (or the group) is working there. A leading `/` lists
 * the commands (a group's are its bots'); in a group, typing `@` suggests its bots. While `reply` is set, the
 * message it quotes sits at the top of the box and goes out with the next send. Under an artifact, `about` is the
 * artifact: the box keeps a draft of its own, and every message replies to the artifact's card.
 */
export function Composer({ botId, reply = null, about = null, cancelReply, onCall, onInterrupt }: {
  botId: string
  reply?: Entry | null
  about?: Artifact | null
  cancelReply?: () => void
  /** Starts a voice call; while the box is empty it takes the send button's place. */
  onCall?: (() => void) | null
  /** Interrupts the bot while it is speaking in a voice call. */
  onInterrupt?: (() => void) | null
}) {
  const store = useStore()
  // Remount transient editor state (selection, attachments, IME) when its destination changes.
  return <DestinationComposer key={JSON.stringify([store.contextId, store.computer.id, botId, about?.id])}
    botId={botId} reply={reply} about={about} cancelReply={cancelReply} onCall={onCall} onInterrupt={onInterrupt} />
}

function DestinationComposer({ botId, reply, about, cancelReply, onCall, onInterrupt }: {
  botId: string; reply: Entry | null; about: Artifact | null; cancelReply?: () => void; onCall?: (() => void) | null; onInterrupt?: (() => void) | null
}) {
  const store = useStore()
  const tr = useTr()
  const plugins = useModel(pluginsFor(store))
  const bot = store.bots.get(botId) ?? null
  const drafts = store.composerDrafts
  const draftKey = about ? artifactDraftKey(about) : botId
  const draft = useSyncExternalStore(drafts.subscribe, () => drafts.get(draftKey))
  const setDraft = (text: string) => drafts.set(draftKey, text)
  const [files, setFiles] = useState<OutgoingFile[]>([])
  const dictation = useDictation((change) => drafts.set(draftKey, change(drafts.get(draftKey))))
  const field = useRef<HTMLTextAreaElement>(null)
  const measure = useRef<HTMLDivElement>(null)
  const composing = useRef(false)
  /** Text pasted into this draft since the last send. */
  const pasted = useRef<string[]>([])
  const working = !!bot && isWorkingIn(bot, botId)
  const empty = !draft.trim() && !files.length
  const canSend = !empty && (store.connection.kind === 'online' || (store.canQueue && !files.length))
  const canAttach = !!bot

  const members = bot && isGroup(bot) ? store.members(bot) : []

  const name = bot?.name ?? ''
  const placeholder = about
    ? tr('Message about “{title}”').replace('{title}', about.title)
    : reply
    ? tr('Reply…')
    : bot && isGroup(bot)
      ? tr('Ask everyone · @ to ask one bot')
      : working
        ? tr('Queue a message for {name}').replace('{name}', name)
        : tr('Ask {name}').replace('{name}', name)

  // A routine's "Try it" fills the chat's box (an artifact's leaves it for the chat).
  const routineDraft = about ? undefined : store.routineDrafts.get(botId)
  useEffect(() => {
    if (routineDraft === undefined) return
    const incoming = store.consumeRoutineDraft(botId)
    if (incoming === undefined) return
    const previous = drafts.get(botId)
    drafts.set(botId, previous ? `${previous}\n${incoming}` : incoming)
    field.current?.focus()
  }, [routineDraft, botId, store, drafts])

  // Picking a message to reply to brings the caret here.
  useEffect(() => {
    if (reply) field.current?.focus()
  }, [reply])

  // Measure a separate, wrapping copy: collapsing the live textarea to zero breaks its
  // height transition, caret scroll and IME composition. Observe font/width changes too.
  useLayoutEffect(() => {
    const el = field.current
    const mirror = measure.current
    if (!el || !mirror) return
    const resize = () => {
      const style = getComputedStyle(el)
      const line = parseFloat(style.lineHeight)
      const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom)
      const height = Math.min(mirror.getBoundingClientRect().height, line * 8 + padding)
      el.style.height = `${height}px`
    }
    resize()
    const observer = new ResizeObserver(resize)
    observer.observe(mirror)
    return () => observer.disconnect()
  }, [])

  const mentionQuery = (() => {
    if (!bot || !isGroup(bot)) return null
    const at = draft.lastIndexOf('@')
    if (at < 0) return null
    if (at > 0 && !/\s/.test(draft[at - 1]!)) return null
    const query = draft.slice(at + 1)
    return /\n/.test(query) || query.length > 24 ? null : query
  })()
  const suggestions: Bot[] =
    mentionQuery !== null ? members.filter((m) => !mentionQuery || m.name.toLowerCase().includes(mentionQuery.toLowerCase())) : []

  const mention = (member: Bot) => {
    const at = draft.lastIndexOf('@')
    if (at < 0) return
    setDraft(`${draft.slice(0, at)}@${member.name} `)
    field.current?.focus()
  }

  const slash = slashQuery(draft)
  const commands = slash === null ? [] : slashCommands(bot, slash, plugins.installedSkills, members)

  const run = (command: SlashCommand) => {
    if (!bot) return
    switch (command.action.kind) {
      case 'fast':
        if (isGroup(bot)) {
          // Each member's own Fast mode: all on, or all back off.
          const fast = groupFastMode(members)
          for (const m of fast?.members ?? []) store.setFastMode(m, fast?.state === 'off')
        } else {
          store.setFastMode(bot, !bot.fastMode)
        }
        setDraft('')
        break
      case 'skill':
        setDraft(`/${command.action.id} `)
        break
    }
    field.current?.focus()
  }
  const menu = useCommandMenu(draft, commands, run)

  const append = (incoming: { name: string; data: Uint8Array }[]) => {
    if (!canAttach) return
    const accepted: OutgoingFile[] = []
    for (const f of incoming) {
      if (f.data.length > MAX_FILE_SIZE) {
        store.setError(`${f.name} is larger than 100 MB.`)
        continue
      }
      accepted.push({ id: crypto.randomUUID(), name: f.name, data: f.data })
    }
    setFiles((list) => [...list, ...accepted])
  }

  const submit = () => {
    const command = files.length ? null : builtInCommand(bot, draft, members)
    if (!command && !canSend) return
    // Words still on their way would land in the emptied box (and the shortcut's tap would send them again).
    dictation.cancel()
    if (command) return run(command)
    store.send(draft, botId, { files, pasted: pasted.current, replyTo: reply ? quoteOf(reply) : about && artifactQuote(about) })
    cancelReply?.()
    pasted.current = []
    setDraft('')
    setFiles([])
    field.current?.focus()
  }
  useVoiceShortcut(dictation, draft, submit, () => field.current?.focus())

  const trailing: Trailing = onInterrupt && !draft && !files.length ? 'interrupt' : working && !draft && !files.length ? 'stop' : onCall && !draft && !files.length ? 'call' : 'send'
  const [symbol, label] = ({
    interrupt: ['stop.fill', 'Interrupt'],
    stop: ['stop.fill', 'Stop'],
    call: ['waveform', 'Call'],
    send: ['arrow.up', 'Send'],
  } as const)[trailing]
  const enabled = trailing !== 'send' || canSend

  return (
    <VStack>
      <ComposerReveal open={suggestions.length > 0}>
        <div className="mentions">
          {suggestions.map((m) => (
            <button key={m.id} className="press mention" onClick={() => mention(m)}>
              <BotAvatar bot={m} size={18} animated={false} />
              <Text size="subheadline" tone="text">{m.name}</Text>
            </button>
          ))}
        </div>
      </ComposerReveal>
      <div
        className="composer"
        onDragOver={(e) => {
          if (canAttach && e.dataTransfer.types.includes('Files')) e.preventDefault()
        }}
        onDrop={(e) => {
          if (!canAttach) return
          e.preventDefault()
          void Promise.all([...e.dataTransfer.files].map(async (f) => ({ name: f.name, data: new Uint8Array(await f.arrayBuffer()) }))).then(append)
        }}
      >
        <div className="composer-surface">
          <CommandMenu commands={menu.shown} index={menu.index} hover={menu.hover} pick={run} />
          <ComposerReveal open={!!reply}>
            {reply ? (
              <HStack gap={4} className="composer-reply">
                <QuoteRow quote={quoteOf(reply)} className="composer-reply-quote" />
                <IconButton title="Cancel reply" icon="xmark" size={24} onClick={cancelReply} />
              </HStack>
            ) : null}
          </ComposerReveal>
          <ComposerReveal open={files.length > 0}>
            <div className="file-chips">
              {files.map((f) => (
                <ComposerAttachment key={f.id} file={f} remove={() => setFiles((list) => list.filter((x) => x.id !== f.id))} />
              ))}
            </div>
          </ComposerReveal>
          <div className="composer-box">
            {/* One capsule: attachments, the field, and send / stop / call. */}
            {canAttach ? (
              <button className="press composer-add" aria-label="Add files" title="Add files" onClick={() => void window.codync.app.pickFiles().then(append)}>
                <Icon name="plus" size={13} weight="medium" color="var(--text)" />
              </button>
            ) : null}
            <div className="composer-field">
              <div ref={measure} className="composer-measure" aria-hidden="true">{draft + '\u200b'}</div>
              <textarea
                ref={field}
                rows={1}
                value={draft}
                placeholder={placeholder}
                spellCheck
                aria-label={reply ? 'Reply' : 'Message'}
                aria-keyshortcuts="Enter Shift+Enter"
                onCompositionStart={() => { composing.current = true }}
                onCompositionEnd={() => { composing.current = false }}
                onChange={(e) => {
                  setDraft(e.target.value)
                  // Starting a command: list the computer's skills again.
                  if (bot && e.target.value === '/') void plugins.refresh()
                }}
                onKeyDown={(e) => {
                  if (menu.onKey(e, composing.current || e.nativeEvent.isComposing || e.keyCode === 229)) return
                  if (e.key === 'Escape' && reply) {
                    e.preventDefault()
                    cancelReply?.()
                    return
                  }
                  // Return sends, Shift-Return adds a line; an input method's Return picks its candidate.
                  const action = composerEnter({ key: e.key, shiftKey: e.shiftKey, altKey: e.altKey, repeat: e.repeat, isComposing: composing.current || e.nativeEvent.isComposing, keyCode: e.keyCode })
                  if (e.key === 'Enter' && e.repeat && !e.shiftKey && !e.altKey) e.preventDefault()
                  if (action === 'send') {
                    e.preventDefault()
                    submit()
                  }
                }}
                onPaste={(e) => {
                  const types = [...e.clipboardData.types]
                  const hasFiles = types.includes('Files')
                  // Pasted text is sent marked as pasted: memory never takes it as the user's own words.
                  if (types.includes('text/plain') && !e.clipboardData.files.length) pasted.current.push(e.clipboardData.getData('text/plain'))
                  // ⌘V with a copied file or image: attach it (the field alone would paste a name).
                  if (!canAttach) return
                  if (!hasFiles || (types.includes('text/plain') && !e.clipboardData.files.length)) return
                  e.preventDefault()
                  void window.codync.app.readClipboardFiles().then((picked) => {
                    if (picked.length) return append(picked)
                    return Promise.all([...e.clipboardData.files].map(async (f) => ({ name: f.name || `pasted-${Date.now()}.png`, data: new Uint8Array(await f.arrayBuffer()) }))).then(append)
                  })
                }}
              />
            </div>
            {dictation.listening || dictation.transcribing ? (
              <RecordingPill
                startedAt={dictation.startedAt}
                sample={() => dictation}
                onStop={() => dictation.finish()}
                transcribing={dictation.transcribing}
                className="composer-recording"
              />
            ) : dictation.available ? (
              <HStack as="span" gap={0} className="composer-mic-group">
                <button className="press composer-mic" aria-label="Dictate" title="Dictate" onClick={() => dictation.start(draft)}>
                  <Icon name="mic" size={13} weight="medium" />
                </button>
                <DictationEngineMenu />
              </HStack>
            ) : null}
            <button
              className={trailing === 'interrupt' ? 'press composer-send interrupt' : 'press composer-send'}
              disabled={!enabled}
              aria-label={label}
              title={label}
              onClick={() => {
                if (trailing === 'interrupt') onInterrupt?.()
                else if (trailing === 'stop') store.stop(botId)
                else if (trailing === 'call') onCall?.()
                else submit()
              }}
            >
              {(['stop.fill', 'waveform', 'arrow.up'] as const).map((name) => (
                <span key={name} aria-hidden="true" className={`composer-symbol ${name === symbol ? 'active' : ''}`}>
                  <Icon name={name} size={name === 'stop.fill' ? 12 : name === 'waveform' ? 15 : 14} weight="bold" />
                </span>
              ))}
            </button>
            {bot && !isGroup(bot) ? <ContextUsageIndicator usage={bot.contextUsage} /> : null}
          </div>
        </div>
      </div>
    </VStack>
  )
}

/** Keep the outgoing row until its height/fade transition finishes. */
function ComposerReveal({ open, children }: { open: boolean; children: ReactNode }) {
  const { mounted, shown } = usePresence(open)
  const last = useRef(children)
  if (open) last.current = children
  return <div className={`composer-reveal ${shown ? 'shown' : ''}`} inert={!open}>
    <div>{mounted ? (open ? children : last.current) : null}</div>
  </div>
}
