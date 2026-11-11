import { useState } from 'react'
import { fromProfile, teamProfiles, type BotCatalog, type BotProfile, type TeamProfile } from '@shared/catalog'
import { CharacterAvatar } from '../../components/Avatar'
import { CardForm, CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { errorText, fillDefaults, newDraft } from '../bots/drafts'
import { useCatalog } from '../bots/use-catalog'
import { Field, FormSheet, ItemGrid, MarketRow, SkeletonGrid } from './pieces'

/** Which ready-made catalog an Extensions section lists. */
export type CatalogKind = 'bots' | 'teams'

/** One catalog entry, as its row and its preview show it. */
type Item = { kind: 'bot'; profile: BotProfile } | { kind: 'team'; team: TeamProfile }
const entryKey = (item: Item) => (item.kind === 'bot' ? `bot:${item.profile.id}` : `team:${item.team.id}`)
const titleOf = (item: Item) => (item.kind === 'bot' ? item.profile.name : item.team.name)
const summaryOf = (item: Item) => (item.kind === 'bot' ? item.profile.summary : item.team.summary)

/**
 * One Extensions section of ready-made bots or coordinator teams (host `botCatalog`), filtered by
 * the window's search: adding a bot creates it on the computer's defaults; adding a team creates
 * its coordinator, its teammates and their group (`installTeam`).
 */
export function CatalogPanel(props: {
  kind: CatalogKind
  query: string
  /** Entries added this session, so their row and preview show it. */
  added: string[]
  onAdded: (id: string) => void
  onError: (message: string) => void
}) {
  const store = useStore()
  // A different computer or account starts its own list: nothing of the previous one lingers.
  return <CatalogForComputer key={JSON.stringify([store.contextId, store.computer.id])} {...props} />
}

function CatalogForComputer({ kind, query, added, onAdded, onError }: {
  kind: CatalogKind
  query: string
  added: string[]
  onAdded: (id: string) => void
  onError: (message: string) => void
}) {
  const store = useStore()
  const tr = useTr()
  const { catalog, failure } = useCatalog(store)
  const [busy, setBusy] = useState<string | null>(null)
  const [open, setOpen] = useState<Item | null>(null)
  const q = query.toLowerCase()
  const matches = (name: string, summary: string) => !q || name.toLowerCase().includes(q) || summary.toLowerCase().includes(q)
  const empty = query ? tr('Nothing matches') : tr('Nothing yet.')

  // Offline, or an older host without a catalog: nothing to add, rather than a list that never loads.
  if (!catalog) {
    return store.connection.kind === 'online' && !failure ? <SkeletonGrid /> : <Text tone="secondary">{empty}</Text>
  }

  const items: Item[] = kind === 'bots'
    ? catalog.bots.filter((p) => matches(p.name, p.summary)).map((profile): Item => ({ kind: 'bot', profile }))
    : catalog.teams.filter((t) => matches(t.name, t.summary)).map((team): Item => ({ kind: 'team', team }))

  const add = async (item: Item, roleModels?: Record<string, string>) => {
    const id = entryKey(item)
    setBusy(id)
    try {
      if (item.kind === 'bot') await store.save(fillDefaults(store, fromProfile(newDraft(), item.profile)))
      else await store.installTeam(item.team.id, roleModels)
      onAdded(id)
    } catch (e) {
      onError(errorText(e))
    }
    setBusy(null)
  }

  const row = (item: Item) => {
    const id = entryKey(item)
    return (
      <MarketRow
        key={id}
        title={titleOf(item)}
        subtitle={summaryOf(item)}
        added={added.includes(id)}
        busy={busy === id}
        icon={item.kind === 'bot' ? <ProfileFace profile={item.profile} size={46} /> : <TeamFace catalog={catalog} team={item.team} />}
        onAdd={() => void add(item)}
        onOpen={() => setOpen(item)}
      />
    )
  }

  return (
    <>
      <div className="market-panel-scroll fade-in">
        {items.length ? <ItemGrid>{items.map(row)}</ItemGrid> : <Text tone="secondary">{empty}</Text>}
      </div>
      <FormSheet open={open !== null} onClose={() => setOpen(null)}>
        {open ? (
          <CatalogSheet
            key={entryKey(open)}
            item={open}
            catalog={catalog}
            added={added.includes(entryKey(open))}
            busy={busy === entryKey(open)}
            onAdd={(models) => void add(open, models)}
          />
        ) : null}
      </FormSheet>
    </>
  )
}

function ProfileFace({ profile, size }: { profile: BotProfile; size: number }) {
  return <CharacterAvatar shape={profile.avatarShape} color={profile.avatarColor} size={size} />
}

/** The coordinator in front, its first teammate peeking behind. */
function TeamFace({ catalog, team }: { catalog: BotCatalog; team: TeamProfile }) {
  const [lead, first] = teamProfiles(catalog, team)
  return (
    <HStack as="span" align="flex-end" className="catalog-team-face">
      {first ? <ProfileFace profile={first} size={28} /> : null}
      <ProfileFace profile={lead!} size={36} />
    </HStack>
  )
}

/** A ready-made bot's or team's details: its instructions, or who leads and who is on the team, with Add. */
function CatalogSheet({ item, catalog, added, busy, onAdd }: { item: Item; catalog: BotCatalog; added: boolean; busy: boolean; onAdd: (models?: Record<string, string>) => void }) {
  const tr = useTr()
  const [models, setModels] = useState<Record<string, string>>({})
  const trailing = added ? (
    <span title={tr('Added')} aria-label={tr('Added')} className="skill-added">
      <Icon name="checkmark" size={13} weight="semibold" />
    </span>
  ) : busy ? <Spinner /> : <IconButton title="Add" icon="plus" onClick={() => onAdd(Object.fromEntries(Object.entries(models).filter(([, value]) => value.trim())))} />

  return (
    <SheetPage>
      <ModalHeader icon={item.kind === 'bot' ? <ProfileFace profile={item.profile} size={28} /> : 'person.2'} title={titleOf(item)} trailing={trailing} />
      <CardForm>
        <CardSection>
          <Text size="subheadline" tone="secondary">{summaryOf(item)}</Text>
        </CardSection>
        {item.kind === 'bot' ? (
          <CardSection title="Instructions" footer="You can change them in the bot's settings after adding it.">
            <Text tone="text" className="selectable">{item.profile.instructions}</Text>
          </CardSection>
        ) : (
          <CardSection title="Team" footer="The coordinator leads the group chat and hands each request to the right teammate. Each bot can be changed in its settings.">
            {teamProfiles(catalog, item.team).map((p, index) => (
              <HStack key={p.id} gap={12} align="flex-start">
                <ProfileFace profile={p} size={32} />
                <VStack gap={2} shrink>
                  <Text weight="semibold" tone="text">{index === 0 ? tr('{name} · Coordinator').replaceAll('{name}', p.name) : p.name}</Text>
                  <Text size="caption" tone="secondary">{p.summary}</Text>
                </VStack>
              </HStack>
            ))}
          </CardSection>
        )}
        {item.kind === 'team' && catalog.capabilities?.roleModels ? (
          <CardSection title="Models by role" footer="Optional model IDs for the host's selected agent. Leave blank for its default; availability and cost depend on that agent.">
            {teamProfiles(catalog, item.team).map((profile) => <VStack key={profile.id} gap={4}>
              <Text size="footnote">{profile.name}</Text>
              <Field label={`${profile.name} model ID`} placeholder="Default" value={models[profile.id] ?? ''}
                onChange={(value) => setModels((current) => ({ ...current, [profile.id]: value }))} />
            </VStack>)}
          </CardSection>
        ) : null}
      </CardForm>
    </SheetPage>
  )
}
