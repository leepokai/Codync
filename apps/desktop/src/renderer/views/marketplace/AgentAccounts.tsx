import { useEffect, useState } from 'react'
import type { AgentAccount, Backend } from '@shared/models'
import { Button, CardSection, IconButton, Spinner } from '../../components/Controls'
import { Dialog } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { errorText } from './market-models'

/**
 * The accounts an agent keeps on this computer: the computer's own sign-in and the isolated
 * profiles a bot can pick. Account sign-in can take a few minutes; the host call waits for it.
 */
export function AgentAccounts({ backend, onChanged }: { backend: Backend; onChanged: () => void }) {
  const store = useStore()
  const tr = useTr()
  const host = store.hostName
  const [accounts, setAccounts] = useState<AgentAccount[]>([])
  const [loading, setLoading] = useState(true)
  const [adding, setAdding] = useState(false)
  const [removing, setRemoving] = useState<string | null>(null)
  const [disconnecting, setDisconnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [confirmRemove, setConfirmRemove] = useState<AgentAccount | null>(null)
  const [confirmSignOut, setConfirmSignOut] = useState(false)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    store.agentAccounts(backend.id).then((found) => {
      if (!cancelled) setAccounts(found)
    }).catch((cause: unknown) => {
      if (!cancelled) setError(`Couldn't load accounts: ${errorText(cause)}`)
    }).finally(() => {
      if (!cancelled) setLoading(false)
    })
    return () => { cancelled = true }
  }, [backend.id, store])

  const busy = loading || adding || removing !== null || disconnecting

  const add = async () => {
    setAdding(true)
    setError(null)
    try {
      const account = await store.authenticateAgentAccount(backend.id)
      setAccounts((current) => [...current.filter((item) => item.id !== account.id), account])
      onChanged()
    } catch (cause) {
      setError(`Couldn't add the account: ${errorText(cause)}`)
    }
    setAdding(false)
  }

  const remove = async (account: AgentAccount) => {
    setConfirmRemove(null)
    setRemoving(account.id)
    setError(null)
    try {
      await store.deleteAgentAccount(account.id)
      setAccounts((current) => current.filter((item) => item.id !== account.id))
      onChanged()
    } catch (cause) {
      setError(`Couldn't remove the account: ${errorText(cause)}`)
    }
    setRemoving(null)
  }

  const signOut = async () => {
    setConfirmSignOut(false)
    setDisconnecting(true)
    setError(null)
    try {
      await store.signOutAgent(backend.id)
      onChanged()
    } catch (cause) {
      setError(`Couldn't sign out: ${errorText(cause)}`)
    }
    setDisconnecting(false)
  }

  const defaultDetail = backend.signedIn === true
    ? (backend.email ? `Signed in as ${backend.email} on ${host}.` : `Signed in on ${host}.`)
    : backend.signedIn === false ? `Not signed in on ${host}.`
    : `Couldn't tell whether ${host} is signed in.`

  return (
    <CardSection title={tr('Accounts')} footer={`Each account keeps its own sign-in on ${host}. Bots can use the default account or any isolated one.`}>
      <HStack gap={10}>
        <VStack grow gap={2}>
          <Text size="body" weight="medium" tone="text">{tr('Default account')}</Text>
          <Text size="subheadline" tone="secondary" className="selectable">{defaultDetail}</Text>
        </VStack>
        {backend.signOut ? (
          <Button kind="secondary" disabled={busy} onClick={() => setConfirmSignOut(true)}>
            {disconnecting ? 'Signing out...' : 'Disconnect'}
          </Button>
        ) : null}
      </HStack>
      {loading ? (
        <HStack gap={8}>
          <Spinner size={12} />
          <Text size="subheadline" tone="secondary">Loading accounts...</Text>
        </HStack>
      ) : accounts.map((account) => (
        <HStack key={account.id} gap={10}>
          <VStack grow gap={2}>
            <Text size="body" weight="medium" tone="text">{account.name}</Text>
            <Text size="subheadline" tone="secondary">Isolated account on {host}.</Text>
          </VStack>
          <IconButton
            title={`Remove ${account.name}`}
            icon="trash"
            disabled={busy}
            onClick={() => setConfirmRemove(account)}
          />
        </HStack>
      ))}
      <HStack gap={10}>
        <VStack grow gap={2}>
          <Text size="body" weight="medium" tone="text">{tr('Add account')}</Text>
          <Text size="subheadline" tone="secondary">
            {adding ? `Finish signing in in the browser on ${host}...` : 'Sign in to another account without signing out of this one.'}
          </Text>
        </VStack>
        <Button disabled={busy} onClick={() => { void add() }}>
          {adding ? 'Signing in...' : '+ ' + tr('Add account')}
        </Button>
      </HStack>
      {error ? <Text size="caption" tone="attention" className="selectable">{error}</Text> : null}
      <Dialog
        open={confirmRemove !== null}
        title="Remove account?"
        message={confirmRemove ? `${confirmRemove.name} will be signed out and its saved sign-in removed from ${host}. Bots using it will need another account.` : null}
        actions={[{ title: 'Remove', destructive: true, action: () => { if (confirmRemove) void remove(confirmRemove) } }]}
        onClose={() => setConfirmRemove(null)}
      />
      <Dialog
        open={confirmSignOut}
        title={`Sign out of ${backend.name}?`}
        message={`This removes the computer's own sign-in on ${host}, as if you no longer have this account. Isolated accounts are not affected.`}
        actions={[{ title: 'Sign out', destructive: true, action: () => { void signOut() } }]}
        onClose={() => setConfirmSignOut(false)}
      />
    </CardSection>
  )
}
