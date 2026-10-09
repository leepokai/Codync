import { useCallback, useEffect, useRef, useState } from 'react'
import { draftOf, type BotDraft, type DirListing } from '@shared/models'
import { CharacterAvatar } from '../../components/Avatar'
import { CardSection, ChoicePicker, DropdownMenu, IconButton, Pill, SearchField, Spinner, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, Sheet, useDismiss } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { AVATAR_COLORS, AVATAR_SHAPES } from '../../lib/theme'
import { useStore } from '../../store/context'
import { AgentModelPicker } from './AgentModelPicker'
import { errorText, fillDefaults, isValid, lastPathComponent, normalized, usePlugins, type Plugins } from './drafts'
import { MemoryCard } from './MemoryView'
import { AnchoredPanel, AutoTextArea, Field, SwitchRow } from './parts'
import './bots.css'

/** Create or edit a bot in a modal: the settings form plus Close / Save. */
export function BotEditorView({ draft: initial }: { draft: BotDraft }) {
  const store = useStore()
  const dismiss = useDismiss()
  const plugins = usePlugins(store)
  const isNew = !initial.id
  const [draft, setDraft] = useState(() => (isNew ? fillDefaults(store, initial, []) : initial))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const online = store.connection.kind === 'online'

  // A new bot's connectors start on once the computer has listed them.
  useEffect(() => {
    if (isNew && plugins) setDraft((d) => fillDefaults(store, d, plugins.connectors))
  }, [isNew, plugins, store])

  const save = () => {
    if (!isValid(draft) || saving || !online) return
    setSaving(true)
    setError(null)
    store
      .save(normalized(draft))
      .then((bot) => {
        dismiss()
        if (isNew) store.setSelection(bot.id)
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setSaving(false))
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <ModalHeader
        title={isNew ? 'New bot' : 'Settings'}
        trailing={saving ? <Spinner /> : <IconButton title={isNew ? 'Create' : 'Save'} icon="checkmark" disabled={!isValid(draft) || saving || !online} onClick={save} />}
      />
      <div style={{ flex: 1, minHeight: 0 }}>
        <BotSettingsForm draft={draft} onChange={setDraft} error={error} plugins={plugins} />
      </div>
    </div>
  )
}

/** The desktop inspector next to a conversation: the same form, saved as you edit. */
export function BotSettingsPanel({ botId }: { botId: string }) {
  const store = useStore()
  const plugins = usePlugins(store)
  const [draft, setDraft] = useState<BotDraft | null>(null)
  const [error, setError] = useState<string | null>(null)
  const change = useCallback((update: (d: BotDraft) => BotDraft) => setDraft((d) => (d ? update(d) : d)), [])

  useEffect(() => {
    const bot = store.bots.get(botId)
    setDraft(bot ? draftOf(bot) : null)
    // Only a different bot reloads the form; the host's echo of a save must not undo typing.
  }, [botId])

  // Autosave a moment after the last change.
  useEffect(() => {
    const bot = store.bots.get(botId)
    if (!draft || !bot || JSON.stringify(draft) === JSON.stringify(draftOf(bot)) || !isValid(draft)) return
    const timer = setTimeout(() => {
      store
        .save(normalized(draft))
        .then(() => setError(null))
        .catch((e: unknown) => setError(errorText(e)))
    }, 600)
    return () => clearTimeout(timer)
  }, [draft])

  return draft ? <BotSettingsForm draft={draft} onChange={change} error={error} plugins={plugins} /> : null
}

/** Bot settings: a big avatar (Grok Bot's), then sections in the Settings style (`CardSection`). */
function BotSettingsForm({ draft, onChange, error, plugins }: {
  draft: BotDraft
  onChange: (update: (d: BotDraft) => BotDraft) => void
  error: string | null
  plugins: Plugins | null
}) {
  const store = useStore()
  const [pickingAvatar, setPickingAvatar] = useState(false)
  const [pickingFolder, setPickingFolder] = useState(false)
  const avatarRef = useRef<HTMLButtonElement>(null)
  const set = <K extends keyof BotDraft>(key: K, value: BotDraft[K]) => onChange((d) => ({ ...d, [key]: value }))
  const setModel = useCallback((model: string | null) => onChange((d) => ({ ...d, model })), [onChange])

  const backends = store.hello?.backends ?? []
  const backend = backends.find((b) => b.id === draft.backend)
  const connectors = plugins?.connectors ?? []
  const skills = plugins?.skills ?? []

  return (
    <div className="bot-form">
      <div className="bot-form-content">
        <div style={{ display: 'flex', justifyContent: 'center' }}>
          <button ref={avatarRef} className="avatar-button" title="Edit Bot avatar" aria-label="Edit Bot avatar" onClick={() => setPickingAvatar((p) => !p)}>
            <CharacterAvatar shape={draft.avatarShape} color={draft.avatarColor} size={56} />
          </button>
          <AnchoredPanel open={pickingAvatar} onClose={() => setPickingAvatar(false)} anchor={avatarRef}>
            <div style={{ padding: 14, width: 300 }}>
              <AvatarPicker shape={draft.avatarShape} color={draft.avatarColor} onShape={(s) => set('avatarShape', s)} onColor={(c) => set('avatarColor', c)} />
            </div>
          </AnchoredPanel>
        </div>

        <CardSection title="Profile">
          {/* A new bot is named from its first conversations (Grok Bot's flow); rename it any time after. */}
          {draft.id ? (
            <Field label="Name">
              <input className="field-box" value={draft.name} placeholder="Name" onChange={(e) => set('name', e.target.value)} />
            </Field>
          ) : null}
          <Field label="Standing instructions" detail="Rules that always apply. Put task-specific requests in the chat instead.">
            <AutoTextArea className="field-box" value={draft.description} placeholder="e.g. Reviews PRs. Never pushes without asking." minRows={3} maxRows={8} onChange={(v) => set('description', v)} />
          </Field>
        </CardSection>

        <CardSection title="Agent">
          <ValueRow label="Computer">
            <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{store.hostName}</span>
          </ValueRow>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <ValueRow label="Agent">
              <ChoicePicker
                selection={draft.backend}
                options={[...backends.map((b) => ({ id: b.id, label: b.available ? b.name : `${b.name} (not installed)` })), { id: 'custom', label: 'Custom command' }]}
                onChange={(id) => set('backend', id)}
              />
            </ValueRow>
            {backend && !backend.available ? <span style={{ ...font('caption'), color: 'var(--warning)' }}>{backend.installHint}</span> : null}
            {draft.backend === 'custom' ? (
              <input
                className="field-box fade-in"
                style={font('callout', undefined, 'monospaced')}
                value={draft.command ?? ''}
                placeholder="ACP command, e.g. my-agent --acp"
                spellCheck={false}
                onChange={(e) => set('command', e.target.value || null)}
              />
            ) : null}
          </div>
          <AgentModelPicker backend={draft.backend} selection={draft.model} onChange={setModel} />
          <ValueRow
            label="Workspace"
            detail={
              draft.cwd
                ? 'This project is the default starting folder. The bot can work elsewhere when you ask.'
                : 'This bot has its own space for files. It can work in other folders when you ask.'
            }
          >
            <DropdownMenu
              title={draft.cwd || 'A persistent workspace allocated for this bot'}
              items={() => [
                { title: 'Personal workspace', selected: !draft.cwd, action: () => set('cwd', '') },
                { title: 'Choose project folder…', selected: !!draft.cwd, action: () => setPickingFolder(true) },
              ]}
            >
              <Pill outlined style={{ gap: 6, color: 'var(--text)' }}>
                <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', maxWidth: 160 }}>{draft.cwd ? lastPathComponent(draft.cwd) : 'Personal'}</span>
                <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
              </Pill>
            </DropdownMenu>
          </ValueRow>
          <ValueRow
            label="Permissions"
            detail={
              draft.permission === 'auto'
                ? "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply."
                : "You'll get an approval card and a notification whenever the agent asks."
            }
          >
            <ChoicePicker
              selection={draft.permission}
              options={[
                { id: 'ask', label: 'Ask me' },
                { id: 'auto', label: 'Approve automatically' },
              ]}
              onChange={(id) => set('permission', id)}
            />
          </ValueRow>
        </CardSection>

        <CardSection title="Activity">
          <SwitchRow title="Notifications" detail="Get notified when this bot finishes or needs you" on={draft.notify ?? true} onChange={(on) => set('notify', on)} />
          {store.screen ? (
            <SwitchRow
              title="Use the computer"
              detail={
                store.screen.platform !== 'windows' && !(store.screen.enabled && store.screen.connected && store.screen.capture && store.screen.input)
                  ? 'Let this bot use apps. On that computer, open Codync → Settings → Computer access to set up permissions first.'
                  : store.screen.enabled
                  ? 'Let this bot see the screen and use the mouse and keyboard. You can watch and take over from your phone.'
                  : store.screen.computerUse
                    ? 'Let this bot see the screen and use the mouse and keyboard.'
                    : "Let this bot see the screen and use the mouse and keyboard. On that computer, open Codync → Settings → Computer access to set up permissions first."
              }
              on={draft.computer ?? false}
              onChange={(on) => set('computer', on)}
            />
          ) : null}
        </CardSection>

        <PluginToggles
          title="Connectors"
          empty="No connectors yet. Add GitHub, Linear, Notion and more from Plugins."
          items={connectors.map((c) => ({ id: c.id, name: c.name, detail: c.command ?? c.url ?? '' }))}
          selection={draft.connectors ?? []}
          onChange={(ids) => set('connectors', ids)}
        />
        <PluginToggles
          title="Skills"
          empty="No skills yet. Get some from Plugins, or write your own."
          items={skills.map((s) => ({ id: s.id, name: s.name, detail: s.description }))}
          selection={draft.skills ?? []}
          onChange={(ids) => set('skills', ids)}
        />
        {draft.id ? <MemoryCard botId={draft.id} /> : null}

        {error ? <span className="fade-in" style={{ ...font('footnote'), color: 'var(--danger)' }}>{error}</span> : null}
      </div>
      <Sheet open={pickingFolder} onClose={() => setPickingFolder(false)} width={460} height={520}>
        <FolderPicker
          start={draft.cwd || store.hello?.home || null}
          onPick={(path) => {
            set('cwd', path)
            setPickingFolder(false)
          }}
        />
      </Sheet>
    </div>
  )
}

/** A card of on/off switches for the connectors or skills installed on the computer. */
function PluginToggles({ title, empty, items, selection, onChange }: {
  title: string
  empty: string
  items: { id: string; name: string; detail: string }[]
  selection: string[]
  onChange: (ids: string[]) => void
}) {
  return (
    <CardSection title={title}>
      {items.length ? null : <span style={{ color: 'var(--secondary)' }}>{empty}</span>}
      {items.map((item) => (
        <SwitchRow
          key={item.id}
          title={item.name}
          detail={item.detail || null}
          on={selection.includes(item.id)}
          onChange={(on) => onChange([...selection.filter((id) => id !== item.id), ...(on ? [item.id] : [])])}
        />
      ))}
    </CardSection>
  )
}

function AvatarPicker({ shape, color, onShape, onColor }: { shape: string; color: string; onShape: (s: string) => void; onColor: (c: string) => void }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div className="swatch-row">
        {AVATAR_SHAPES.map((s) => (
          <button key={s} className={`swatch ${s === shape ? 'on' : ''}`} aria-label={`${s} shape`} aria-pressed={s === shape} onClick={() => onShape(s)}>
            <CharacterAvatar shape={s} color={color} size={36} />
          </button>
        ))}
      </div>
      <div className="swatch-row">
        {AVATAR_COLORS.map((c) => (
          <button key={c.id} className={`swatch ${c.id === color ? 'on' : ''}`} aria-label={c.label} aria-pressed={c.id === color} onClick={() => onColor(c.id)}>
            <span style={{ width: 26, height: 26, borderRadius: '50%', background: c.hex, display: 'block' }} />
          </button>
        ))}
      </div>
    </div>
  )
}

/** Browses folders on the host, drilling down in place. */
function FolderPicker({ start, onPick }: { start: string | null; onPick: (path: string) => void }) {
  // The folders opened so far; the last one is shown.
  const [stack, setStack] = useState<(string | null)[]>([start])
  const [forward, setForward] = useState(true)
  const current = stack[stack.length - 1] ?? null
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <div style={{ display: 'flex', alignItems: 'center' }}>
        {stack.length > 1 ? (
          <div className="fade-in" style={{ paddingLeft: 10, display: 'flex' }}>
            <IconButton
              title="Back"
              icon="chevron.left"
              onClick={() => {
                setForward(false)
                setStack((s) => s.slice(0, -1))
              }}
            />
          </div>
        ) : null}
        <div style={{ flex: 1, minWidth: 0 }}>
          <ModalHeader title={current ? lastPathComponent(current) : 'Folders'} />
        </div>
      </div>
      <div style={{ flex: 1, minHeight: 0, overflow: 'hidden' }}>
        <FolderLevel
          key={stack.length}
          path={current}
          forward={forward}
          onPick={onPick}
          onOpen={(path) => {
            setForward(true)
            setStack((s) => [...s, path])
          }}
        />
      </div>
    </div>
  )
}

/** One folder's subfolders. */
function FolderLevel({ path, forward, onPick, onOpen }: { path: string | null; forward: boolean; onPick: (path: string) => void; onOpen: (path: string) => void }) {
  const store = useStore()
  const [listing, setListing] = useState<DirListing | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [filter, setFilter] = useState('')

  useEffect(() => {
    let live = true
    store
      .listDirs(path)
      .then((l) => live && setListing(l))
      .catch((e: unknown) => live && setError(errorText(e)))
    return () => {
      live = false
    }
  }, [store, path])

  const dirs = listing?.dirs.filter((d) => !filter || d.name.toLowerCase().includes(filter.toLowerCase())) ?? []

  return (
    <div className={`bot-form level-slide ${forward ? 'from-trailing' : 'from-leading'}`}>
      <div className="bot-form-content" style={{ gap: 14 }}>
        <SearchField value={filter} onChange={setFilter} />
        {listing ? (
          <>
            <CardSection footer={listing.path}>
              <button className="press" style={{ display: 'flex', alignItems: 'center', gap: 8, ...font('headline'), color: 'var(--accent)' }} onClick={() => onPick(listing.path)}>
                <Icon name="checkmark.circle.fill" size={13} />
                <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{`Use “${lastPathComponent(listing.path)}”`}</span>
              </button>
            </CardSection>
            {dirs.length ? (
              <CardSection>
                {dirs.map((dir) => (
                  <button key={dir.path} className="press" style={{ display: 'flex', alignItems: 'center', gap: 10 }} onClick={() => onOpen(dir.path)}>
                    <span style={{ width: 20, display: 'flex', justifyContent: 'center' }}>
                      <Icon name={dir.isGit ? 'arrow.triangle.branch' : 'folder'} size={12} color={dir.isGit ? 'var(--accent)' : 'var(--secondary)'} />
                    </span>
                    <span style={{ flex: 1, minWidth: 0, color: 'var(--text)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{dir.name}</span>
                    <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
                  </button>
                ))}
              </CardSection>
            ) : null}
          </>
        ) : error ? (
          <span style={{ color: 'var(--danger)' }}>{error}</span>
        ) : (
          <div style={{ display: 'flex', justifyContent: 'center' }}>
            <Spinner size={20} />
          </div>
        )}
      </div>
    </div>
  )
}
