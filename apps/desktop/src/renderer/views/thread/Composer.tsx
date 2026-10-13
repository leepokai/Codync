import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react'
import { isBelow } from '@shared/compat'
import { isGroup, isWorkingIn, type Bot } from '@shared/models'
import { BotAvatar } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { usePresence } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { useModel } from '../../lib/observable'
import { useStore } from '../../store/context'
import { MAX_FILE_SIZE, type OutgoingFile } from '../../store/bot-store'
import { pluginsFor } from '../marketplace/plugins'
import { composerEnter } from './composer-keyboard'
import { ComposerAttachment } from './ComposerAttachment'
import { matchingSkills, SKILLS_HOST, skillQuery } from './skill-command'

type Trailing = 'interrupt' | 'stop' | 'call' | 'send'

/**
 * The message box under a chat or a thread: send, or stop while the bot (or the group) is
 * working there. In a group, typing `@` suggests its bots; with one bot, a leading `/` suggests its skills.
 */
export function Composer({ botId, thread = null, onCall, onInterrupt }: {
  botId: string
  thread?: string | null
  /** Starts a voice call; while the box is empty it takes the send button's place. */
  onCall?: (() => void) | null
  /** Interrupts the bot while it is speaking in a voice call. */
  onInterrupt?: (() => void) | null
}) {
  const store = useStore()
  // Remount transient editor state (selection, attachments, IME) when its destination changes.
  return <DestinationComposer key={JSON.stringify([store.contextId, store.computer.id, botId, thread])}
    botId={botId} thread={thread} onCall={onCall} onInterrupt={onInterrupt} />
}

function DestinationComposer({ botId, thread, onCall, onInterrupt }: {
  botId: string; thread: string | null; onCall?: (() => void) | null; onInterrupt?: (() => void) | null
}) {
  const store = useStore()
  const plugins = useModel(pluginsFor(store))
  const bot = store.bots.get(botId) ?? null
  const drafts = store.composerDrafts
  const draft = useSyncExternalStore(drafts.subscribe, () => drafts.get(botId, thread))
  const setDraft = (text: string) => drafts.set(botId, thread, text)
  const [files, setFiles] = useState<OutgoingFile[]>([])
  const field = useRef<HTMLTextAreaElement>(null)
  const measure = useRef<HTMLDivElement>(null)
  const composing = useRef(false)
  const working = !!bot && isWorkingIn(bot, botId, thread)
  const empty = !draft.trim() && !files.length
  const canSend = !empty && (store.connection.kind === 'online' || (store.canQueue && !files.length))
  const canAttach = !!bot

  const placeholder = thread ? 'Reply…' : bot && isGroup(bot) ? 'Message everyone · @ to ask one bot' : working ? `Queue a message for ${bot?.name ?? ''}` : `Ask ${bot?.name ?? ''}`

  // A routine's "Try it" fills the box.
  const routineDraft = store.routineDrafts.get(botId)
  useEffect(() => {
    if (thread || routineDraft === undefined) return
    const incoming = store.consumeRoutineDraft(botId)
    if (incoming === undefined) return
    const previous = drafts.get(botId, thread)
    drafts.set(botId, thread, previous ? `${previous}\n${incoming}` : incoming)
    field.current?.focus()
  }, [routineDraft, thread, botId, store, drafts])

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
    mentionQuery !== null && bot ? store.members(bot).filter((m) => !mentionQuery || m.name.toLowerCase().includes(mentionQuery.toLowerCase())) : []

  const mention = (member: Bot) => {
    const at = draft.lastIndexOf('@')
    if (at < 0) return
    setDraft(`${draft.slice(0, at)}@${member.name} `)
    field.current?.focus()
  }

  // Not in a group: each member has its own skills.
  const skillsShown = !!bot && !isGroup(bot) && !isBelow(store.hello?.version ?? '', SKILLS_HOST)
  const typedSkill = skillsShown ? skillQuery(draft) : null
  const skills = typedSkill === null || !bot ? [] : matchingSkills(plugins.installedSkills, bot.skills, typedSkill)

  const pickSkill = (id: string) => {
    setDraft(`/${id} `)
    field.current?.focus()
  }

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
    if (!canSend) return
    store.send(draft, botId, thread, files)
    setDraft('')
    setFiles([])
    field.current?.focus()
  }

  const trailing: Trailing = onInterrupt && !draft && !files.length ? 'interrupt' : working && !draft && !files.length ? 'stop' : onCall && !draft && !files.length ? 'call' : 'send'
  const [symbol, label] = ({
    interrupt: ['stop.fill', 'Interrupt'],
    stop: ['stop.fill', 'Stop'],
    call: ['waveform', 'Call'],
    send: ['arrow.up', 'Send'],
  } as const)[trailing]
  const fill = trailing === 'interrupt' ? 'var(--danger)' : trailing === 'send' ? (canSend ? 'var(--accent-fill)' : 'var(--accent-dim)') : 'var(--accent-fill)'
  const ink = trailing === 'interrupt' ? '#fff' : trailing === 'send' ? (canSend ? 'var(--on-accent)' : 'var(--tertiary)') : 'var(--on-accent)'
  const enabled = trailing !== 'send' || canSend

  return (
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      <ComposerReveal open={suggestions.length > 0 || skills.length > 0}>
        <div className="mentions">
          {suggestions.map((m) => (
            <button key={m.id} className="press mention" onClick={() => mention(m)}>
              <BotAvatar bot={m} size={18} animated={false} />
              <span style={{ ...font('subheadline'), color: 'var(--text)' }}>{m.name}</span>
            </button>
          ))}
          {skills.map((s) => (
            <button key={s.id} className="press mention" title={s.description || s.name} onClick={() => pickSkill(s.id)}>
              <Icon name="book.pages" size={12} color="var(--secondary)" />
              <span style={{ ...font('subheadline'), color: 'var(--text)' }}>{s.name}</span>
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
                aria-label={thread ? 'Reply' : 'Message'}
                aria-keyshortcuts="Enter Shift+Enter"
                onCompositionStart={() => { composing.current = true }}
                onCompositionEnd={() => { composing.current = false }}
                onChange={(e) => {
                  setDraft(e.target.value)
                  // Starting a skill name: list the computer's skills again.
                  if (skillsShown && e.target.value === '/') void plugins.refresh()
                }}
                onKeyDown={(e) => {
                  // Return sends, Shift-Return adds a line; an input method's Return picks its candidate.
                  const action = composerEnter({ key: e.key, shiftKey: e.shiftKey, altKey: e.altKey, repeat: e.repeat, isComposing: composing.current || e.nativeEvent.isComposing, keyCode: e.keyCode })
                  if (e.key === 'Enter' && e.repeat && !e.shiftKey && !e.altKey) e.preventDefault()
                  if (action === 'send') {
                    e.preventDefault()
                    submit()
                  }
                }}
                onPaste={(e) => {
                  // ⌘V with a copied file or image: attach it (the field alone would paste a name).
                  if (!canAttach) return
                  const types = [...e.clipboardData.types]
                  const hasFiles = types.includes('Files')
                  if (!hasFiles || (types.includes('text/plain') && !e.clipboardData.files.length)) return
                  e.preventDefault()
                  void window.codync.app.readClipboardFiles().then((picked) => {
                    if (picked.length) return append(picked)
                    return Promise.all([...e.clipboardData.files].map(async (f) => ({ name: f.name || `pasted-${Date.now()}.png`, data: new Uint8Array(await f.arrayBuffer()) }))).then(append)
                  })
                }}
              />
            </div>
            <button
              className="press composer-send"
              disabled={!enabled}
              aria-label={label}
              title={label}
              style={{ background: fill, color: ink }}
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
    </div>
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
