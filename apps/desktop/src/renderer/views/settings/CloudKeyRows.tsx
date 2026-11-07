import { useState } from 'react'
import { Button, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { CloudCredential, CloudKeySetup } from '../../lib/cloud-computer'
import { useTr } from '../../lib/use-language'
import type { CloudComputerModel } from '../../store/cloud-computer'

/**
 * Codync Cloud → the key to the user's own Boat account (iOS `CloudKeyRows`): pasted once, checked and kept encrypted
 * by the cloud, and shown afterwards only by its last four. While Boat turns it down, what to fix shows under it.
 */
export function CloudKeyRows({ model, canRemove }: { model: CloudComputerModel; canRemove: boolean }) {
  const [replacing, setReplacing] = useState(false)
  const credential = model.credential
  if (credential && !replacing) return <SavedKey credential={credential} model={model} canRemove={canRemove} replace={() => setReplacing(true)} />
  return <KeyEntry setup={model.keySetup} model={model} cancel={replacing ? () => setReplacing(false) : null} />
}

function SavedKey({ credential, model, canRemove, replace }: { credential: CloudCredential; model: CloudComputerModel; canRemove: boolean; replace: () => void }) {
  const tr = useTr()
  return (
    <VStack gap={6} className="settings-row">
      <HStack gap={10}>
        <Icon name="key.fill" size={13} color={credential.problem ? 'var(--danger)' : 'var(--secondary)'} />
        <VStack grow className="settings-stack">
          <Text tone="text" truncate>{tr('Boat API key')}</Text>
          <Text size="caption" tone="tertiary" mono truncate>{credential.hint}</Text>
        </VStack>
        {model.busy === 'removeKey' ? (
          <Spinner />
        ) : (
          <>
            <IconButton title="Replace key" icon="pencil" onClick={replace} disabled={model.busy !== null} />
            {canRemove ? <IconButton title="Remove key" icon="trash" onClick={() => void model.removeKey()} disabled={model.busy !== null} /> : null}
          </>
        )}
      </HStack>
      {credential.problem ? <Text size="caption" tone="danger">{tr(credential.problem)}</Text> : null}
    </VStack>
  )
}

/** How to make the key, then a field to paste it. */
function KeyEntry({ setup, model, cancel }: { setup: CloudKeySetup | null; model: CloudComputerModel; cancel: (() => void) | null }) {
  const tr = useTr()
  const [key, setKey] = useState('')
  const [copied, setCopied] = useState(false)
  const saving = model.busy === 'saveKey'
  const save = async () => {
    if (!key.trim() || model.busy) return
    if (await model.saveKey(key)) {
      setKey('')
      cancel?.()
    }
  }
  return (
    <VStack gap={10} className="settings-row">
      {setup ? (
        <>
          <Text size="footnote" tone="secondary">
            {tr('Make a key limited to Codync in your Boat account, then paste it here. Codync keeps it encrypted and never shows it again.')}
          </Text>
          <HStack gap={8} align="start" className="settings-commands">
            <Text size="caption" mono grow className="settings-commands-text">{setup.commands}</Text>
            <IconButton
              title={copied ? 'Copied' : 'Copy'}
              icon={copied ? 'checkmark' : 'doc.on.doc'}
              onClick={() => {
                window.codync.app.copy(setup.commands)
                setCopied(true)
              }}
            />
          </HStack>
        </>
      ) : null}
      <HStack gap={8}>
        <input
          type="password"
          aria-label={tr('Boat API key')}
          placeholder="boat_…"
          value={key}
          spellCheck={false}
          autoComplete="off"
          className="settings-key-input"
          onChange={(e) => setKey(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && void save()}
        />
        {saving ? <Spinner /> : <Button onClick={() => void save()} disabled={!key.trim() || model.busy !== null}>Save</Button>}
      </HStack>
      <Text as="div" size="footnote" className="settings-row">
        {setup ? (
          <button className="settings-link" onClick={() => window.codync.app.openExternal(setup.page)}>
            {tr('Open Boat')}
          </button>
        ) : null}
        <Spacer />
        {cancel ? (
          <button className="settings-quiet-button" onClick={() => { setKey(''); cancel() }}>
            {tr('Cancel')}
          </button>
        ) : null}
      </Text>
    </VStack>
  )
}
