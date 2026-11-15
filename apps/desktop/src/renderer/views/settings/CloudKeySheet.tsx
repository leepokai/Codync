import { useState } from 'react'
import { Button, CardSection, Disclosure, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { CloudCredential, CloudProvider, CloudProviderOffer } from '../../lib/cloud-computer'
import { fill } from '../../lib/fill'
import { useTr } from '../../lib/use-language'
import type { CloudComputerModel } from '../../store/cloud-computer'

/**
 * One provider's key (iOS `CloudKeySheet`), from New cloud computer or a cloud computer's `⋯`: the key to the user's own
 * account there, pasted once, checked and kept encrypted by the cloud, and shown afterwards only by its last four; while
 * the provider turns it down, what to fix shows under it. Saving one closes it (`done`).
 */
export function CloudKeySheet({ model, provider, done }: { model: CloudComputerModel; provider: string; done: () => void }) {
  const tr = useTr()
  const p = model.providers.find((x) => x.provider === provider)
  if (!p) return null
  return (
    <VStack className="cloud-key-sheet">
      <ModalHeader
        icon="key.fill"
        title={fill(tr('{provider} API key'), { provider: p.name })}
        subtitle={tr('Your cloud computer runs on your own account. Keys stay encrypted; only their last four show.')}
      />
      <VStack gap={20} className="cloud-key-body">
        <CardSection>
          <KeyRow provider={p} model={model} saved={done} />
          {p.offer ? <OfferRow provider={p.provider} offer={p.offer} model={model} /> : null}
          <SetupRow provider={p} />
        </CardSection>
        <ErrorText error={model.errorAt('card')} />
      </VStack>
    </VStack>
  )
}

/** Why the last action failed. */
function ErrorText({ error }: { error: string | null }) {
  const tr = useTr()
  return error ? <Text size="caption" tone="danger">{tr(error)}</Text> : null
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

/** How to make the key, folded away: the cloud's own steps, to copy, or the provider's page. */
function SetupRow({ provider }: { provider: CloudProvider }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const [copied, setCopied] = useState(false)
  return (
    <Disclosure expanded={open} onToggle={() => setOpen((o) => !o)} label={<Text tone="text">{tr('How to make a key')}</Text>}>
      <VStack gap={10}>
        <Text size="footnote" tone="secondary">{fill(tr('Make a key with only what Codync needs in your {provider} account, then paste it above.'), { provider: provider.name })}</Text>
        <HStack gap={8} align="start" className="settings-commands">
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
        <HStack>
          <button className="settings-link" onClick={() => window.codync.app.openExternal(provider.setup.page)}>
            <Text size="footnote">{fill(tr('Open {provider}'), { provider: provider.name })}</Text>
          </button>
          <Spacer />
        </HStack>
      </VStack>
    </Disclosure>
  )
}
