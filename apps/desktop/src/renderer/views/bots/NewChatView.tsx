import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { draftOf, isGroup, type Bot } from '@shared/models'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { createDefaultBot, errorText } from './drafts'
import { AutoTextArea, BotChip } from './parts'
import './bots.css'

/**
 * Desktop "new message" page (Grok Bot's compose): a To: field that searches your
 * bots or creates a new one, with ⌘1…⌘9 shortcuts, and the message box underneath.
 * Picked bots become chips in the To: field; one opens its chat, several start a
 * group chat with them (or open the one they already share). Whatever you typed is sent.
 * `group` is "New group chat": the same page, but it only gathers bots and needs two.
 */
export function NewChatView({ group, onClose }: { group: boolean; onClose: () => void }) {
  const store = useStore()
  const [query, setQuery] = useState('')
  const [draft, setDraft] = useState('')
  const [recipients, setRecipients] = useState<string[]>([])
  const [creating, setCreating] = useState(false)
  const toField = useRef<HTMLInputElement>(null)
  const chips = useRef<HTMLDivElement>(null)

  // Bots not picked yet; a group can only be opened on its own, so groups go once someone's picked.
  const q = query.trim().toLowerCase()
  const matches = store.roster.filter((b) => !recipients.includes(b.id) && !(isGroup(b) && (group || recipients.length > 0)) && (!q || b.name.toLowerCase().includes(q)))
  const picked = recipients.map((id) => store.bots.get(id)).filter((b): b is Bot => !!b)

  const toPrompt = group ? (recipients.length < 2 ? 'Add bots to the group' : 'Add another bot') : recipients.length ? 'Add another bot' : 'Search or create bots'
  const placeholder = group
    ? picked.length < 2
      ? 'Pick at least two bots'
      : 'Message the group'
    : picked.length === 0
      ? 'Message Bot'
      : `Message ${picked.map((b) => b.name).join(', ')}`

  useEffect(() => {
    toField.current?.focus()
  }, [])

  // The field stays in view as chips are added.
  useLayoutEffect(() => {
    const el = chips.current
    if (el) el.scrollTo({ left: el.scrollWidth, behavior: 'smooth' })
  }, [recipients])

  const open = (id: string) => {
    const text = draft.trim()
    store.setSelection(id)
    if (text) store.send(text, id)
    onClose()
  }

  // A bot becomes a chip; a group (with nobody picked) opens right away.
  const choose = (bot: Bot) => {
    if (isGroup(bot)) return open(bot.id)
    setRecipients((list) => [...list, bot.id])
    setQuery('')
    toField.current?.focus()
  }

  const remove = (id: string) => setRecipients((list) => list.filter((r) => r !== id))

  // One bot: its chat. Several: their group chat.
  const start = () => {
    if (recipients.length === 0) return
    if (recipients.length === 1) {
      if (!group) open(recipients[0]!)
      return
    }
    store
      .createGroup(picked.map((b) => b.name).join(', '), '', recipients)
      .then((g) => open(g.id))
      .catch((e: unknown) => store.setError(errorText(e)))
  }

  const create = () => {
    if (creating) return
    setCreating(true)
    const name = query.trim()
    const alone = recipients.length === 0 && !group
    void (async () => {
      try {
        let bot = await createDefaultBot(store)
        if (name) bot = await store.save({ ...draftOf(bot), name })
        if (alone) {
          open(bot.id)
        } else {
          // `createDefaultBot` selected it; stay here and add it to the others.
          store.setSelection(null)
          choose(bot)
        }
      } catch (e) {
        store.setError(errorText(e))
      }
      setCreating(false)
    })()
  }

  // Return in To: picks the top match, or with the field empty, starts the chat.
  const submitTo = () => {
    if (q) {
      if (matches[0]) choose(matches[0])
      else create()
    } else {
      start()
    }
  }

  // ⌘1 creates, ⌘2…⌘9 pick the matches; Escape closes.
  const keys = useRef({ create, choose, matches, onClose })
  keys.current = { create, choose, matches, onClose }
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        keys.current.onClose()
        return
      }
      if (!e.metaKey || e.shiftKey || e.altKey || e.ctrlKey) return
      const n = Number(e.key)
      if (!Number.isInteger(n) || n < 1 || n > 9) return
      e.preventDefault()
      if (n === 1) keys.current.create()
      else {
        const bot = keys.current.matches[n - 2]
        if (bot) keys.current.choose(bot)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <div className="drag" style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '12px 22px', ...font('title3') }}>
        <span style={{ color: 'var(--secondary)', flex: 'none' }}>To:</span>
        <div ref={chips} className="chip-row no-drag" style={{ flex: 1 }}>
          {picked.map((bot) => (
            <BotChip key={bot.id} bot={bot} onRemove={() => remove(bot.id)} />
          ))}
          <input
            ref={toField}
            value={query}
            placeholder={toPrompt}
            style={{ minWidth: 200, flex: 1, color: 'var(--text)', userSelect: 'text' }}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing || e.keyCode === 229) return
              if (e.key === 'Enter') {
                e.preventDefault()
                submitTo()
              } else if (e.key === 'Backspace' && !query && recipients.length) {
                // Backspace in an empty field takes the last chip back out.
                e.preventDefault()
                remove(recipients[recipients.length - 1]!)
              }
            }}
          />
        </div>
        <IconButton title="Close" icon="xmark" className="no-drag" onClick={onClose} />
      </div>
      <div className="hairline" />

      <div className="compose-list">
        <PickRow shortcut={1} onClick={create} icon={
          <span style={{ width: 26, height: 26, borderRadius: '50%', background: 'var(--bubble-agent)', display: 'grid', placeItems: 'center' }}>
            <Icon key={creating ? 'hourglass' : 'plus'} name={creating ? 'hourglass' : 'plus'} size={13} weight="medium" className="fade-in" />
          </span>
        }>
          {query ? `Create “${query}”` : 'Create new Bot'}
        </PickRow>
        {matches.map((bot, i) => (
          <PickRow key={bot.id} shortcut={i + 2 <= 9 ? i + 2 : null} onClick={() => choose(bot)} icon={isGroup(bot) ? <GroupAvatar members={store.members(bot)} size={26} animated={false} /> : <BotAvatar bot={bot} size={26} animated={false} />}>
            {bot.name}
          </PickRow>
        ))}
      </div>

      <div style={{ flex: 1 }} />

      <div className="compose-box">
        <AutoTextArea
          value={draft}
          onChange={setDraft}
          placeholder={placeholder}
          minRows={1}
          maxRows={6}
          onKeyDown={(e) => {
            // Return sends, Shift-Return adds a line; an input method's Return picks its candidate.
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing && e.keyCode !== 229) {
              e.preventDefault()
              start()
            }
          }}
        />
      </div>
    </div>
  )
}

function PickRow({ shortcut, onClick, icon, children }: { shortcut: number | null; onClick: () => void; icon: ReactNode; children: ReactNode }) {
  return (
    <button className="press pick-row hover fade-in" onClick={onClick}>
      {icon}
      <span style={{ flex: 1, minWidth: 0, color: 'var(--text)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{children}</span>
      {shortcut !== null ? (
        <span style={{ display: 'flex', gap: 3 }}>
          <span className="key-cap">⌘</span>
          <span className="key-cap">{shortcut}</span>
        </span>
      ) : null}
    </button>
  )
}
