import { useState } from 'react'
import { Button, DropdownMenu, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, useDismiss } from '../../components/Overlay'
import { ProviderIcon } from '../../components/ProviderIcon'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { chosenProvider, limitCloudName, limitNote, providerOption, suggestedName, type CloudProvider } from '../../lib/cloud-computer'
import { fill } from '../../lib/fill'
import { useTr } from '../../lib/use-language'
import type { CloudComputerModel } from '../../store/cloud-computer'
import '../bots/agent-picker.css'

/**
 * Computers → Codync Cloud → "+": the new cloud computer's name (the suggested one when left empty) and the provider it
 * runs on, every provider listed with its logo like the agents; one without a usable key opens that provider's key
 * (`openKey`) to add it, and one with a key opens it, or removes it, from its row's `⋯`. At the account's limit it says
 * so, and its keys stay in reach.
 */
export function NewCloudComputerSheet({ model, openKey }: { model: CloudComputerModel; openKey: (provider: string) => void }) {
  const tr = useTr()
  const dismiss = useDismiss()
  const [name, setName] = useState('')
  const [picked, setPicked] = useState<string | null>(null)
  const provider = chosenProvider(model.providers, picked)
  const suggested = suggestedName(model.machines, tr)
  // A key removed from a row's `⋯` fails here, where it was asked.
  const error = model.errorAt('create') ?? model.keyRemovalError ?? limitNote(model.machines.length, model.limit)
  const create = async () => {
    if (!provider || !model.canCreate) return
    if (await model.create(provider, name.trim() || suggested)) dismiss()
  }
  return (
    <VStack className="cloud-new-sheet">
      <ModalHeader icon="cloud" title={tr('New cloud computer')} />
      <VStack gap={16} className="cloud-new-body">
        <VStack gap={6}>
          <Text size="caption" weight="semibold" tone="secondary">{tr('Name')}</Text>
          <input
            className="dialog-field"
            data-autofocus
            value={name}
            placeholder={suggested}
            aria-label={tr('Name your cloud computer')}
            spellCheck={false}
            onChange={(e) => setName(limitCloudName(e.target.value))}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.nativeEvent.isComposing && e.keyCode !== 229) void create()
            }}
          />
        </VStack>
        <VStack gap={6}>
          <Text size="caption" weight="semibold" tone="secondary">{tr('Provider')}</Text>
          <VStack gap={2}>
            {model.providers.map((p) => (
              <ProviderRow key={p.provider} provider={p} model={model} selected={p.provider === provider} pick={() => setPicked(p.provider)} openKey={() => openKey(p.provider)} />
            ))}
          </VStack>
        </VStack>
        {error ? <Text size="footnote" tone="danger" className="fade-in">{tr(error)}</Text> : null}
      </VStack>
      <HStack gap={8} className="cloud-new-footer">
        <Spacer />
        <Button kind="secondary" onClick={dismiss}>Cancel</Button>
        <Button onClick={() => void create()} disabled={!provider || !model.canCreate}>{model.busy === 'create' ? <Spinner /> : 'Create'}</Button>
      </HStack>
    </VStack>
  )
}

/** A provider on its logo tile with where it runs and its key; picked when its key is usable, else picked for once it is
 * and it opens its key. One with a key has a `⋯` in the same row: its key (replace it or see what to fix), and removing
 * it while no cloud computer runs there. */
function ProviderRow({ provider, model, selected, pick, openKey }: { provider: CloudProvider; model: CloudComputerModel; selected: boolean; pick: () => void; openKey: () => void }) {
  const tr = useTr()
  const option = providerOption(provider, tr)
  const removing = model.busy === `removeKey:${provider.provider}`
  return (
    <HStack className="cloud-provider-row">
      <button
        className="agent-picker-item"
        aria-pressed={option.usable ? selected : undefined}
        onClick={() => {
          pick()
          if (!option.usable) openKey()
        }}
      >
        <span className="cloud-provider-tile">
          <ProviderIcon provider={provider.provider} size={16} />
        </span>
        <VStack as="span" gap={1} grow>
          <Text tone="text" truncate>{provider.name}</Text>
          <Text size="caption" tone={option.detail.error ? 'danger' : 'secondary'} lines={2}>{option.detail.text}</Text>
        </VStack>
        {selected ? <Icon name="checkmark" size={12} /> : null}
      </button>
      {!option.usable ? null : removing ? (
        <span className="cloud-provider-menu"><Spinner /></span>
      ) : (
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
      )}
    </HStack>
  )
}
