import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react'
import { groupFastMode } from '@shared/fast-mode'
import { artifactDraftKey, artifactQuote, type Artifact } from '@shared/artifacts'
import { isGroup, isWorkingIn, type Bot, type ReplyQuote } from '@shared/models'
import { Icon } from '../../components/Icon'
import { IconButton } from '../../components/Controls'
import { RecordingPill } from '../../components/RecordingPill'
import { Sheet, usePresence } from '../../components/Overlay'
import { VoiceChatSettingsView } from '../settings/VoiceChatSettingsView'
import { HStack, VStack } from '../../components/Stack'
import { useModel } from '../../lib/observable'
import type { OutgoingFile } from '../../store/outgoing-files'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { pluginsFor } from '../marketplace/plugins'
import { ComposerField } from './ComposerField'
import { CommandMenu, MentionMenu, useComposerMenu } from './ComposerMenu'
import { ComposerAttachment } from './ComposerAttachment'
import { useComposerFiles } from './use-composer-files'
import { QuoteRow } from './ChatRows'
import { useDictation } from './dictation-platform'
import { DictationEngineMenu } from './DictationEngineMenu'
import { mentionSuggestions, returnMentions, withMention } from './mentions'
import { builtInCommand, returnRuns, slashCommands, slashQuery, type SlashCommand } from './slash-commands'
import { useSubmitAfterRender, useVoiceShortcut } from './voice-shortcut-keys'

type Trailing = 'interrupt' | 'stop' | 'call' | 'send'

/**
 * The message box under a chat: send, or stop while the bot (or the group) is working there. A leading `/` lists
 * the commands (a group's are its bots'); in a group, typing `@` lists its bots. While `reply` is set, the
 * message it quotes sits at the top of the box and goes out with the next send. Under an artifact, `about` is the
 * artifact: the box keeps a draft of its own, and every message replies to the artifact's card.
 */
export function Composer({ botId, reply = null, about = null, cancelReply, onCall, onInterrupt }: {
  botId: string
  reply?: ReplyQuote | null
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
  botId: string; reply: ReplyQuote | null; about: Artifact | null; cancelReply?: () => void; onCall?: (() => void) | null; onInterrupt?: (() => void) | null
}) {
  const store = useStore()
  const tr = useTr()
  const plugins = useModel(pluginsFor(store))
  const bot = store.bots.get(botId) ?? null
  const drafts = store.composerDrafts
  const draftKey = about ? artifactDraftKey(about) : botId
  const draft = useSyncExternalStore(drafts.subscribe, () => drafts.get(draftKey))
  const setDraft = (text: string) => drafts.set(draftKey, text)
  const [voiceSettings, setVoiceSettings] = useState(false)
  const dictation = useDictation((change) => drafts.set(draftKey, change(drafts.get(draftKey))), () => setVoiceSettings(true))
  const field = useRef<HTMLTextAreaElement>(null)
  /** Text pasted into this draft since the last send. */
  const pasted = useRef<string[]>([])
  const working = !!bot && isWorkingIn(bot, botId)
  const canAttach = !!bot
  const attach = useComposerFiles(canAttach)
  const files = attach.files
  const empty = !draft.trim() && !files.length
  const canSend = !empty && (store.connection.kind === 'online' || (store.canQueue && !files.length))

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

  // Picking a message to reply to brings the caret here.
  useEffect(() => {
    if (reply) field.current?.focus()
  }, [reply])

  const suggestions = mentionSuggestions(draft, members)
  const mention = (member: Bot) => {
    setDraft(withMention(draft, member))
    field.current?.focus()
  }
  const mentionMenu = useComposerMenu(draft, suggestions, mention, returnMentions)

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
      case 'agent':
        setDraft(`/${command.name} `)
        break
    }
    field.current?.focus()
  }
  const commandMenu = useComposerMenu(draft, commands, run, returnRuns)

  const submit = () => {
    const command = files.length ? null : builtInCommand(bot, draft, members)
    if (!command && !canSend) return
    // Sent once the words still on their way are in the box, with what was typed.
    if (dictation.busy) return dictation.finish(submitLater)
    if (command) return run(command)
    store.send(draft, botId, { files, pasted: pasted.current, replyTo: reply ?? (about && artifactQuote(about)) })
    cancelReply?.()
    pasted.current = []
    setDraft('')
    attach.clear()
    field.current?.focus()
  }
  const submitLater = useSubmitAfterRender(submit)
  useVoiceShortcut(dictation, draft, submitLater, () => field.current?.focus())

  const trailing: Trailing = onInterrupt && !draft && !files.length ? 'interrupt' : working && !draft && !files.length ? 'stop' : onCall && !draft && !files.length ? 'call' : 'send'
  const [symbol, label] = ({
    interrupt: ['stop.fill', 'Interrupt'],
    stop: ['stop.fill', tr('Stop')],
    call: ['waveform', tr('Call')],
    send: ['arrow.up', 'Send'],
  } as const)[trailing]
  const enabled = trailing !== 'send' || canSend

  return (
    <VStack>
      <div className="composer" {...attach.drop}>
        <div className="composer-surface">
          <CommandMenu menu={commandMenu} agent={{ registry: (id) => store.agentRegistry(id), name: (id) => store.backendName(id) }} />
          <MentionMenu menu={mentionMenu} />
          <ComposerReveal open={!!reply}>
            {reply ? (
              <HStack gap={4} className="composer-reply">
                <QuoteRow quote={reply} className="composer-reply-quote" />
                <IconButton title="Cancel reply" icon="xmark" size={24} onClick={cancelReply} />
              </HStack>
            ) : null}
          </ComposerReveal>
          <ComposerFiles files={files} remove={attach.remove} />
          <div className="composer-box">
            {/* One capsule: attachments, the field, and send / stop / call. */}
            {canAttach ? (
              <AddFilesButton pick={attach.pick} />
            ) : null}
            <ComposerField
              field={field}
              value={draft}
              placeholder={placeholder}
              label={tr(reply ? 'Reply' : 'Message')}
              onChange={(text) => {
                setDraft(text)
                // Starting a command: list the computer's skills again.
                if (bot && text === '/') void plugins.refresh()
              }}
              onSend={submit}
              onKey={(e, inInputMethod) => {
                if (commandMenu.onKey(e, inInputMethod) || mentionMenu.onKey(e, inInputMethod)) return true
                if (e.key !== 'Escape' || !reply) return false
                e.preventDefault()
                cancelReply?.()
                return true
              }}
              onPaste={(e) => {
                // Pasted text is sent marked as pasted: memory never takes it as the user's own words.
                if (e.clipboardData.types.includes('text/plain') && !e.clipboardData.files.length) pasted.current.push(e.clipboardData.getData('text/plain'))
                attach.paste(e)
              }}
            />
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
                <button className="press composer-mic" aria-label={tr('Dictate')} title={tr('Dictate')} onClick={() => dictation.start(draft)}>
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
          </div>
        </div>
      </div>
      <Sheet open={voiceSettings} onClose={() => setVoiceSettings(false)}>
        <VoiceChatSettingsView />
      </Sheet>
    </VStack>
  )
}

/** The files going with the next message, each with its remove button. */
export function ComposerFiles({ files, remove }: { files: OutgoingFile[]; remove: (id: string) => void }) {
  return (
    <ComposerReveal open={files.length > 0}>
      <div className="file-chips">
        {files.map((f) => <ComposerAttachment key={f.id} file={f} remove={() => remove(f.id)} />)}
      </div>
    </ComposerReveal>
  )
}

/** The box's + : pick files to send. */
export function AddFilesButton({ pick }: { pick: () => void }) {
  const tr = useTr()
  return (
    <button className="press composer-add" aria-label={tr('Add files')} title={tr('Add files')} onClick={pick}>
      <Icon name="plus" size={13} weight="medium" color="var(--text)" />
    </button>
  )
}

/** Keep the outgoing row until its height/fade transition finishes. */
export function ComposerReveal({ open, children }: { open: boolean; children: ReactNode }) {
  const { mounted, shown } = usePresence(open)
  const last = useRef(children)
  if (open) last.current = children
  return <div className={`composer-reveal ${shown ? 'shown' : ''}`} inert={!open}>
    <div>{mounted ? (open ? children : last.current) : null}</div>
  </div>
}
