import { useState, type ReactNode } from 'react'
import { CardForm, CardSection, DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage, useDismiss } from '../../components/Overlay'
import { ProviderIcon } from '../../components/ProviderIcon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { isReady, limitCloudName, limitNote, placementLine, providerDetail, suggestedName, type CloudProvider } from '../../lib/cloud-computer'
import { fill } from '@shared/fill'
import { useTr } from '../../lib/use-language'
import type { CloudComputerModel } from '../../store/cloud-computer'
import { CloudKeyContent } from './CloudKeySheet'

/** Where New cloud computer is: picking the provider, then its key (when it has none that works), then the name; a
 * later step's provider by its id, read from the model so a key saved meanwhile shows. */
type Step = { page: 'provider' } | { page: 'key' | 'name'; provider: string }

/**
 * Computers → Codync Cloud → "+" (iOS `NewCloudComputerSheet`), three steps in one sheet, swapped in place: the provider
 * (each with its region, and its key or what it costs; one with a key opens it, or removes it, from its row's `⋯`:
 * `openKey`), its key when it has none that works (`CloudKeyContent`), then the new computer's name (the suggested one
 * when left empty). At the account's limit it says so, and Create is off.
 */
export function NewCloudComputerSheet({ model, openKey }: { model: CloudComputerModel; openKey: (provider: string) => void }) {
  const [step, setStep] = useState<Step>({ page: 'provider' })
  const [name, setName] = useState('')
  // A create that failed was for the step left.
  const go = (next: Step) => {
    model.clearError('create')
    setStep(next)
  }
  const provider = step.page === 'provider' ? null : model.providers.find((p) => p.provider === step.provider)
  if (step.page === 'provider' || !provider) {
    return <ProviderStep model={model} openKey={openKey} pick={(p) => go({ page: isReady(p) ? 'name' : 'key', provider: p.provider })} />
  }
  const back = () => go({ page: 'provider' })
  if (step.page === 'key') return <KeyStep model={model} provider={provider} back={back} saved={() => go({ page: 'name', provider: provider.provider })} />
  return <NameStep model={model} provider={provider} back={back} name={name} setName={setName} />
}

/** A step's page: its title (after the way back to the providers, on the later steps, off while a key saves or the
 * computer is made) and its cards; it fades in as the step changes. */
function StepPage({ model, title, back, trailing, children }: { model: CloudComputerModel; title: string; back?: () => void; trailing?: ReactNode; children: ReactNode }) {
  return (
    <SheetPage className="cloud-new-sheet fade-in">
      <ModalHeader icon={back ? <IconButton title="Back" icon="chevron.left" disabled={model.busy !== null} onClick={back} /> : 'cloud'} title={title} trailing={trailing} />
      <CardForm>{children}</CardForm>
    </SheetPage>
  )
}

/** Why something failed or Create is off, under the step's cards. */
function StepError({ error }: { error: string | null }) {
  const tr = useTr()
  return error ? <Text size="footnote" tone="danger" className="fade-in">{tr(error)}</Text> : null
}

function ProviderStep({ model, openKey, pick }: { model: CloudComputerModel; openKey: (provider: string) => void; pick: (provider: CloudProvider) => void }) {
  const tr = useTr()
  return (
    <StepPage model={model} title={tr('New cloud computer')}>
      <CardSection title="Provider">
        {model.providers.map((p) => (
          <ProviderRow key={p.provider} provider={p} model={model} pick={() => pick(p)} openKey={() => openKey(p.provider)} />
        ))}
      </CardSection>
      {/* A key removed from a row's `⋯` fails here, where it was asked. */}
      <StepError error={model.keyRemovalError ?? limitNote(model.machines.length, model.limit)} />
    </StepPage>
  )
}

/** Its logo on a tile, its name and detail, and a chevron; a `⋯` on one with a key it takes: its key (replace it or see
 * what to fix), and removing it while no cloud computer runs there. */
function ProviderRow({ provider, model, pick, openKey }: { provider: CloudProvider; model: CloudComputerModel; pick: () => void; openKey: () => void }) {
  const tr = useTr()
  const detail = providerDetail(provider, tr)
  return (
    <HStack gap={4} className="cloud-provider-row">
      <button className="cloud-provider-pick" disabled={model.busy !== null} onClick={pick}>
        <span className="cloud-provider-tile">
          <ProviderIcon provider={provider.provider} size={16} />
        </span>
        <VStack as="span" gap={1} grow>
          <Text tone="text" truncate>{provider.name}</Text>
          <Text size="caption" tone={detail.error ? 'danger' : 'secondary'} lines={2}>{detail.text}</Text>
        </VStack>
      </button>
      {model.busy === `removeKey:${provider.provider}` ? (
        <span className="cloud-provider-menu"><Spinner /></span>
      ) : isReady(provider) ? (
        <DropdownMenu
          className="icon-button cloud-provider-menu"
          title={tr('More')}
          disabled={model.busy !== null}
          items={() => [
            { title: fill(tr('{provider} API key'), { provider: provider.name }), verbatim: true, icon: 'key.fill', action: openKey },
            ...(model.canRemoveKey(provider.provider) ? [{ title: 'Remove key', icon: 'trash', destructive: true, action: () => void model.removeKey(provider.provider) }] : []),
          ]}
        >
          <Icon name="ellipsis" size={14} weight="medium" />
        </DropdownMenu>
      ) : null}
      <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
    </HStack>
  )
}

/** The provider's key, with how to make it; once it's kept, on to the name. */
function KeyStep({ model, provider, back, saved }: { model: CloudComputerModel; provider: CloudProvider; back: () => void; saved: () => void }) {
  const tr = useTr()
  return (
    <StepPage model={model} title={fill(tr('Connect {provider}'), { provider: provider.name })} back={back}>
      <CloudKeyContent model={model} provider={provider} saved={saved} />
    </StepPage>
  )
}

/** The new computer's name over where it runs; Create in the header. */
function NameStep({ model, provider, back, name, setName }: { model: CloudComputerModel; provider: CloudProvider; back: () => void; name: string; setName: (name: string) => void }) {
  const tr = useTr()
  const dismiss = useDismiss()
  const suggested = suggestedName(model.machines, tr)
  const ready = model.canCreate && isReady(provider)
  const create = async () => {
    if (!ready) return
    if (await model.create(provider.provider, name.trim() || suggested)) dismiss()
  }
  return (
    <StepPage
      model={model}
      title={tr('New cloud computer')}
      back={back}
      trailing={model.busy === 'create' ? <Spinner /> : <IconButton title="Create" icon="checkmark" disabled={!ready} onClick={() => void create()} />}
    >
      <CardSection title="Name" footer={<Text size="caption" tone="secondary">{placementLine({ providerName: provider.name, region: provider.region }, tr)}</Text>}>
        <input
          autoFocus
          value={name}
          placeholder={suggested}
          aria-label={tr('Name your cloud computer')}
          spellCheck={false}
          onChange={(e) => setName(limitCloudName(e.target.value))}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.nativeEvent.isComposing && e.keyCode !== 229) void create()
          }}
        />
      </CardSection>
      <StepError error={model.errorAt('create') ?? limitNote(model.machines.length, model.limit)} />
    </StepPage>
  )
}
