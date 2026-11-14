import { useState } from 'react'
import type { Backend } from '@shared/models'
import { CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { fill } from '../../lib/fill.ts'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { errorText } from './market-models'
import { ErrorText } from './pieces'

/**
 * An agent's other logins, beside the one the computer already has: add one (its sign-in follows),
 * sign in one that isn't yet, or remove one. Bots pick theirs in their own settings.
 */
export function AgentAccounts({ backend, signIn }: { backend: Backend; signIn: (account: string) => void }) {
  const store = useStore()
  const tr = useTr()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const run = async (key: string, change: () => Promise<void>) => {
    setBusy(key)
    try {
      await change()
      setError(null)
    } catch (e) {
      setError(errorText(e))
    }
    setBusy(null)
  }
  const add = () => run('add', async () => signIn(await store.addAgentAccount(backend.id)))
  const remove = (account: string) => run(account, () => store.removeAgentAccount(account))

  return (
    <CardSection title="Other accounts" footer={`A bot picks its account under Agent in its settings. Bots that pick none use the sign-in above.`}>
      {(backend.accounts ?? []).map((account) => (
        <HStack key={account.id} gap={10} className="agent-step-row">
          <VStack as="span" gap={2} grow>
            <Text size="body" weight="medium" tone="text" truncate>{account.name ?? tr('New account')}</Text>
            <Text size="subheadline" tone="secondary">{tr(account.signedIn ? 'Signed in' : 'Not signed in')}</Text>
          </VStack>
          {account.signedIn ? null : <button className="capsule-action agent-step-action" disabled={busy !== null} onClick={() => signIn(account.id)}>{tr('Sign in')}</button>}
          {busy === account.id ? <Spinner /> : <IconButton title="Remove account" icon="trash" disabled={busy !== null} onClick={() => void remove(account.id)} />}
        </HStack>
      ))}
      <button className="option-row" disabled={busy !== null} onClick={() => void add()}>
        <IconSlot width={26} tone="secondary" aria-hidden>
          <Icon name="plus" size={13} />
        </IconSlot>
        <VStack as="span" gap={2} grow>
          <Text size="body" weight="medium" tone="text">{tr('Add account')}</Text>
          <Text size="subheadline" tone="secondary" lines={3}>{fill(tr('Signs in another {name} account in a terminal on {other}.'), { name: backend.name, other: store.hostName })}</Text>
        </VStack>
        {busy === 'add' ? <Spinner /> : null}
      </button>
      {error ? <ErrorText text={error} /> : null}
    </CardSection>
  )
}
