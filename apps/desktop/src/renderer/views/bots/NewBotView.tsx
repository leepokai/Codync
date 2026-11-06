import { useState, type ReactNode } from 'react'
import { isGroup, type Bot } from '@shared/models'
import { BotAvatar, CharacterAvatar, GroupAvatar } from '../../components/Avatar'
import { Button, ChoicePicker, Spinner } from '../../components/Controls'
import { ModalHeader, useDismiss } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { errorText, fillDefaults, newDraft } from './drafts'
import { AvatarPicker } from './parts'
import './bots.css'

/** Which computer a new bot or group goes to; shown when more than one is online. */
export interface ComputerChoice {
  options: { id: string; label: string }[]
  selection: string
  onChange: (id: string) => void
}

/**
 * A new bot from its looks and an optional name (Grok Bot's "Create New Bot"); the rest starts on
 * the defaults and changes in its Settings. Unnamed, the host names it after its first conversations.
 */
export function NewBotView({ computer }: { computer: ComputerChoice | null }) {
  const store = useStore()
  const [draft, setDraft] = useState(newDraft)

  const create = async () => {
    const bot = await store.save(fillDefaults(store, { ...draft, name: draft.name.trim() }))
    store.setSelection(bot.id)
  }

  return (
    <CreateLayout
      title="New bot"
      preview={<CharacterAvatar shape={draft.avatarShape} color={draft.avatarColor} size={96} />}
      name={draft.name}
      placeholder="Name your bot"
      onName={(name) => setDraft((d) => ({ ...d, name }))}
      canCreate
      computer={computer}
      create={create}
    >
      <AvatarPicker
        shape={draft.avatarShape}
        color={draft.avatarColor}
        onShape={(avatarShape) => setDraft((d) => ({ ...d, avatarShape }))}
        onColor={(avatarColor) => setDraft((d) => ({ ...d, avatarColor }))}
      />
    </CreateLayout>
  )
}

/** A new group chat, laid out like a new bot: its face, an optional name, then the bots to put in it. */
export function NewGroupView({ computer }: { computer: ComputerChoice | null }) {
  const tr = useTr()
  const store = useStore()
  const [name, setName] = useState('')
  const [members, setMembers] = useState<string[]>([])
  const bots = store.roster.filter((b) => !isGroup(b))
  const picked = members.map((id) => store.bots.get(id)).filter((b): b is Bot => !!b)
  // "Alice, Bob" when no name is typed.
  const defaultName = picked.map((b) => b.name).join(', ')

  const toggle = (id: string) => setMembers((list) => (list.includes(id) ? list.filter((m) => m !== id) : [...list, id]))
  const create = async () => {
    await store.createGroup(name.trim() || defaultName, '', members)
  }

  return (
    <CreateLayout
      title="New group chat"
      preview={<GroupAvatar members={picked} size={96} />}
      name={name}
      placeholder={defaultName || 'Name your group'}
      onName={setName}
      canCreate={members.length > 0}
      computer={computer}
      create={create}
    >
      {bots.length ? (
        <HStack wrap justify="center" align="flex-start" gap={4} className="create-members">
          {bots.map((bot) => {
            const on = members.includes(bot.id)
            return (
              <button key={bot.id} className="press create-member" aria-pressed={on} onClick={() => toggle(bot.id)}>
                <span className={`swatch ${on ? 'on' : ''}`}>
                  <BotAvatar bot={bot} size={44} animated={false} />
                </span>
                <Text size="caption" tone={on ? 'text' : 'secondary'} truncate className="create-member-name">{bot.name}</Text>
              </button>
            )
          })}
        </HStack>
      ) : (
        <Text tone="secondary">{tr(`No bots yet. Create a bot first.`)}</Text>
      )}
      <Text as="div" size="caption" tone="tertiary">{tr(`Everyone answers in turn unless you @mention someone.`)}</Text>
    </CreateLayout>
  )
}

/** The create sheet both share: the face, a centered name field, the choices, then Cancel / Create. */
function CreateLayout({ title, preview, name, placeholder, onName, canCreate, computer, create, children }: {
  title: string
  preview: ReactNode
  name: string
  placeholder: string
  onName: (name: string) => void
  canCreate: boolean
  computer: ComputerChoice | null
  create: () => Promise<void>
  children: ReactNode
}) {
  const store = useStore()
  const tr = useTr()
  const dismiss = useDismiss()
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const ready = canCreate && !saving && store.connection.kind === 'online'

  const submit = () => {
    if (!ready) return
    setSaving(true)
    setError(null)
    create()
      .then(dismiss)
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setSaving(false))
  }

  return (
    <VStack className="create-sheet">
      <ModalHeader title={title} />
      <VStack align="center" gap={20} className="create-body">
        {preview}
        <input
          className="field-box create-name"
          autoFocus
          value={name}
          placeholder={tr(placeholder)}
          aria-label="Name"
          onChange={(e) => onName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.nativeEvent.isComposing && e.keyCode !== 229) submit()
          }}
        />
        {children}
        {error ? <Text size="footnote" tone="danger" className="fade-in">{error}</Text> : null}
      </VStack>
      <HStack gap={8} className="create-footer">
        {computer ? (
          <HStack gap={8}>
            <Text tone="secondary">{tr(`On`)}</Text>
            <ChoicePicker selection={computer.selection} options={computer.options} onChange={computer.onChange} />
          </HStack>
        ) : null}
        <Spacer />
        <Button kind="secondary" onClick={dismiss}>Cancel</Button>
        <Button onClick={submit} disabled={!ready}>{saving ? <Spinner /> : 'Create'}</Button>
      </HStack>
    </VStack>
  )
}
