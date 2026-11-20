import { useEffect, useState } from 'react'
import { Button, CardForm, CardSection, Disclosure, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { CloudCredential, CloudProvider, CloudProviderOffer } from '../../lib/cloud-computer'
import { fill } from '@shared/fill'
import { useTr } from '../../lib/use-language'
import type { CloudComputerModel } from '../../store/cloud-computer'

/**
 * One provider's key (iOS `CloudKeySheet`), from a cloud computer's `⋯` or a provider's in New cloud computer: the same
 * key content as New cloud computer's key step. Saving one closes it (`done`).
 */
export function CloudKeySheet({ model, provider, done }: { model: CloudComputerModel; provider: string; done: () => void }) {
  const tr = useTr()
  const p = model.providers.find((x) => x.provider === provider)
  if (!p) return null
  return (
    <SheetPage className="cloud-key-sheet">
      <ModalHeader icon="key.fill" title={fill(tr('{provider} API key'), { provider: p.name })} />
      <CardForm>
        <CloudKeyContent model={model} provider={p} saved={done} />
        <ErrorText error={model.errorAt('card')} />
      </CardForm>
    </SheetPage>
  )
}

/**
 * A provider's key, in its own sheet and as New cloud computer's key step (iOS `CloudKeyForm`): how to make one on the
 * provider's site (numbered, then its page), the key to the user's own account there (pasted once, checked and kept
 * encrypted by the cloud, shown afterwards only by its last four, with what to fix while the provider turns it down),
 * its free plan, and the command line way, folded away. `saved` once a key is kept.
 */
export function CloudKeyContent({ model, provider, saved }: { model: CloudComputerModel; provider: CloudProvider; saved: () => void }) {
  // Left, its failure was seen: New cloud computer shows only a removal asked from a provider's own row.
  useEffect(() => () => model.clearError(`key:${provider.provider}`), [model, provider.provider])
  return (
    <>
      {/* Above the key: the last step says to paste it below. */}
      <SetupSteps provider={provider} />
      <CardSection footer="Your cloud computer runs on your own account. Keys stay encrypted; only their last four show.">
        <KeyRow provider={provider} model={model} saved={saved} />
        {provider.offer ? <OfferRow provider={provider.provider} offer={provider.offer} model={model} /> : null}
        <CommandsRow provider={provider} />
      </CardSection>
    </>
  )
}

/** Why the last action failed. */
function ErrorText({ error }: { error: string | null }) {
  const tr = useTr()
  return error ? <Text size="caption" tone="danger">{tr(error)}</Text> : null
}

/** How to make the key on the provider's own site, numbered, and its page (an older cloud sends no steps, only the page). */
function SetupSteps({ provider }: { provider: CloudProvider }) {
  const tr = useTr()
  const steps = provider.setup.steps ?? []
  return (
    <CardSection>
      {steps.length ? (
        <VStack gap={10}>
          {steps.map((step, i) => (
            <HStack key={step} gap={8} align="baseline">
              <Text tone="tertiary" tabular className="cloud-step-number">{`${i + 1}.`}</Text>
              <Text tone="text" grow>{tr(step)}</Text>
            </HStack>
          ))}
        </VStack>
      ) : null}
      <HStack>
        <button className="settings-link" onClick={() => window.codync.app.openExternal(provider.setup.page)}>
          <Text size="footnote">{fill(tr('Open {provider}'), { provider: provider.name })}</Text>
        </button>
        <Spacer />
      </HStack>
    </CardSection>
  )
}

function KeyRow({ provider, model, saved }: { provider: CloudProvider; model: CloudComputerModel; saved: () => void }) {
  const [replacing, setReplacing] = useState(false)
  const credential = provider.credential
  return (
    <VStack gap={6}>
      {credential && !replacing ? (
        <SavedKey provider={provider} credential={credential} model={model} replace={() => setReplacing(true)} />
      ) : (
        <KeyEntry provider={provider} model={model} cancel={replacing ? () => setReplacing(false) : null} saved={saved} />
      )}
      {/* Under the key it's about: below every provider it would be out of view. */}
      <ErrorText error={model.errorAt(`key:${provider.provider}`)} />
    </VStack>
  )
}

function SavedKey({ provider, credential, model, replace }: { provider: CloudProvider; credential: CloudCredential; model: CloudComputerModel; replace: () => void }) {
  const tr = useTr()
  return (
    <VStack gap={6}>
      <HStack gap={10} className="settings-row">
        <Icon name="key.fill" size={13} color={credential.problem ? 'var(--danger)' : 'var(--secondary)'} />
        <VStack grow className="settings-stack">
          <Text tone="text" truncate>{tr('API key')}</Text>
          <Text size="caption" tone="tertiary" mono truncate>{credential.hint}</Text>
        </VStack>
        {model.busy === `removeKey:${provider.provider}` ? (
          <Spinner />
        ) : (
          <>
            <IconButton title="Replace key" icon="pencil" onClick={replace} disabled={model.busy !== null} />
            {model.canRemoveKey(provider.provider) ? <IconButton title="Remove key" icon="trash" onClick={() => void model.removeKey(provider.provider)} disabled={model.busy !== null} /> : null}
          </>
        )}
      </HStack>
      {credential.problem ? <Text size="caption" tone="danger">{tr(credential.problem)}</Text> : null}
    </VStack>
  )
}

function KeyEntry({ provider, model, cancel, saved }: { provider: CloudProvider; model: CloudComputerModel; cancel: (() => void) | null; saved: () => void }) {
  const tr = useTr()
  const [key, setKey] = useState('')
  const save = async () => {
    if (!key.trim() || model.busy) return
    if (await model.saveKey(provider.provider, key)) {
      setKey('')
      cancel?.()
      saved()
    }
  }
  return (
    <HStack gap={8} className="settings-row">
      <input
        type="password"
        autoFocus
        aria-label={fill(tr('{provider} API key'), { provider: provider.name })}
        placeholder={provider.keyPrompt}
        value={key}
        spellCheck={false}
        autoComplete="off"
        className="settings-key-input"
        onChange={(e) => setKey(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && void save()}
      />
      {cancel ? (
        <button className="settings-quiet-button" onClick={() => { setKey(''); cancel() }}>
          {tr('Cancel')}
        </button>
      ) : null}
      {model.busy === `saveKey:${provider.provider}` ? <Spinner /> : <Button onClick={() => void save()} disabled={!key.trim() || model.busy !== null}>Save</Button>}
    </HStack>
  )
}

/** The provider's own free plan: what it includes, and its page. */
function OfferRow({ provider, offer, model }: { provider: string; offer: CloudProviderOffer; model: CloudComputerModel }) {
  const tr = useTr()
  return (
    <HStack gap={8} className="settings-row">
      <Text size="footnote" tone="secondary" grow>{tr(offer.detail)}</Text>
      <button className="settings-link" onClick={() => model.openOffer(provider, offer.url)}>
        <Text size="footnote">{tr(offer.label)}</Text>
      </button>
    </HStack>
  )
}

/** The provider's CLI way to make the key, folded away: its commands, one a line, to copy. */
function CommandsRow({ provider }: { provider: CloudProvider }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const [copied, setCopied] = useState(false)
  return (
    <Disclosure expanded={open} onToggle={() => setOpen((o) => !o)} label={<Text tone="text">{tr('Use the command line instead')}</Text>}>
      <HStack gap={8} align="start" className="settings-commands fade-in">
        {/* Scrolled sideways rather than wrapped: wrapping breaks them into commands that don't exist. */}
        <Text size="caption" mono grow className="settings-commands-text">{provider.setup.commands}</Text>
        <IconButton
          title={copied ? 'Copied' : 'Copy'}
          icon={copied ? 'checkmark' : 'doc.on.doc'}
          onClick={() => {
            window.codync.app.copy(provider.setup.commands)
            setCopied(true)
          }}
        />
      </HStack>
    </Disclosure>
  )
}
