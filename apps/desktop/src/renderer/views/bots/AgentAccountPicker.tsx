import { useEffect, useRef, useState } from 'react'
import type { AgentAccount } from '@shared/models'
import { Button, ChoicePicker, IconButton, Spinner } from '../../components/Controls'
import { Dialog } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'

interface Props {
  backend: string
  selection?: string | null
  online: boolean
  contextKey: string
  /** The agent can keep isolated accounts beside the computer's own sign-in. */
  supported: boolean
  load: () => Promise<AgentAccount[]>
  authenticate: () => Promise<AgentAccount>
  remove: (accountId: string) => Promise<void>
  /** Signs out of the computer's own account; omitted when the host can't. */
  signOut?: () => Promise<void>
  onChange: (id: string | null) => void
}

/** Account lists belong to one computer; a context change discards their pending results. */
export function AgentAccountPicker(props: Props) {
  if (!props.supported) return null
  return <AccountPicker key={JSON.stringify([props.contextKey, props.backend, props.online])} {...props} />
}

function AccountPicker({ backend, selection, online, load, authenticate, remove, signOut, onChange }: Props) {
  const [accounts, setAccounts] = useState<AgentAccount[]>([])
  const [loading, setLoading] = useState(online)
  const [signingIn, setSigningIn] = useState(false)
  const [removing, setRemoving] = useState<string | null>(null)
  const [signingOut, setSigningOut] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const [retry, setRetry] = useState(0)
  const [confirmRemove, setConfirmRemove] = useState<AgentAccount | null>(null)
  const [confirmSignOut, setConfirmSignOut] = useState(false)
  const alive = useRef(false)
  const callbacks = useRef({ load, authenticate, remove, signOut, onChange })
  callbacks.current = { load, authenticate, remove, signOut, onChange }

  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  useEffect(() => {
    let cancelled = false
    if (!online) return
    setLoading(true)
    setFailure(null)
    void callbacks.current.load().then((value) => {
      if (!cancelled) setAccounts(value.filter((account) => account.backend === backend))
    }).catch((error: unknown) => {
      if (!cancelled) setFailure(`Couldn't load accounts: ${message(error)}`)
    }).finally(() => {
      if (!cancelled) setLoading(false)
    })
    return () => { cancelled = true }
  }, [backend, online, retry])

  const busy = loading || signingIn || removing !== null || signingOut

  const signIn = async () => {
    setSigningIn(true)
    setFailure(null)
    try {
      const account = await callbacks.current.authenticate()
      if (!alive.current) return
      setAccounts((current) => [...current.filter((item) => item.id !== account.id), account])
      callbacks.current.onChange(account.id)
    } catch (error) {
      if (alive.current) setFailure(`Couldn't sign in: ${message(error)}`)
    } finally {
      if (alive.current) setSigningIn(false)
    }
  }

  const removeAccount = async (account: AgentAccount) => {
    setConfirmRemove(null)
    setRemoving(account.id)
    setFailure(null)
    try {
      await callbacks.current.remove(account.id)
      if (!alive.current) return
      setAccounts((current) => current.filter((item) => item.id !== account.id))
      if (selection === account.id) callbacks.current.onChange(null)
    } catch (error) {
      if (alive.current) setFailure(`Couldn't remove the account: ${message(error)}`)
    } finally {
      if (alive.current) setRemoving(null)
    }
  }

  const signOutDefault = async () => {
    setConfirmSignOut(false)
    setSigningOut(true)
    setFailure(null)
    try {
      await callbacks.current.signOut?.()
    } catch (error) {
      if (alive.current) setFailure(`Couldn't sign out: ${message(error)}`)
    } finally {
      if (alive.current) setSigningOut(false)
    }
  }

  const options = [{ id: '', label: 'Default account' }, ...accounts.map((account) => ({ id: account.id, label: account.name }))]
  if (selection && !accounts.some((account) => account.id === selection)) {
    options.push({ id: selection, label: 'Saved account (not loaded)' })
  }
  const selected = accounts.find((account) => account.id === selection)

  return (
    <VStack gap={6}>
      <HStack gap={8}>
        <Text tone="text">Account</Text>
        <Spacer />
        {busy ? <Spinner size={12} /> : (
          <IconButton title={failure ? 'Retry account list' : 'Refresh account list'} icon="arrow.clockwise" disabled={!online} onClick={() => setRetry((value) => value + 1)} />
        )}
        {selection ? (
          <IconButton title="Remove this account" icon="trash" disabled={!online || busy || !selected} onClick={() => selected && setConfirmRemove(selected)} />
        ) : null}
        <ChoicePicker selection={selection ?? ''} options={options} onChange={(id) => onChange(id || null)} />
      </HStack>
      <Button kind="secondary" disabled={!online || busy} onClick={() => { void signIn() }}>
        {signingIn ? 'Signing in…' : 'Sign in to another account'}
      </Button>
      {signOut ? (
        <HStack gap={8}>
          <Text size="caption" tone="secondary" grow>The computer's own sign-in is the default account.</Text>
          <Button kind="secondary" disabled={!online || busy} onClick={() => setConfirmSignOut(true)}>
            {signingOut ? 'Signing out…' : 'Sign out default'}
          </Button>
        </HStack>
      ) : null}
      <div role="status" aria-live="polite">
        {signingIn ? <Text size="caption" tone="secondary">Finish signing in in the browser on this computer.</Text> : null}
        {failure ? <Text size="caption" tone="attention">{`${failure} Retry or sign in again.`}</Text> : !online ? (
          <Text size="caption" tone="secondary">Connect to this computer to load accounts or sign in.</Text>
        ) : !loading && accounts.length === 0 ? (
          <Text size="caption" tone="secondary">No isolated accounts. Use the default account or sign in to add one.</Text>
        ) : null}
      </div>
      <Text size="caption" tone="secondary">Each account keeps its own sign-in on this computer. Changing accounts starts a new agent session; chat history is kept.</Text>
      <Dialog
        open={confirmRemove !== null}
        title="Remove account?"
        message={confirmRemove ? `${confirmRemove.name} will be signed out and its saved sign-in removed from this computer. Bots using it will need another account.` : null}
        actions={[{ title: 'Remove', destructive: true, action: () => { if (confirmRemove) void removeAccount(confirmRemove) } }]}
        onClose={() => setConfirmRemove(null)}
      />
      <Dialog
        open={confirmSignOut}
        title="Sign out of the default account?"
        message="Bots that use the computer's own sign-in will need to sign in again. Isolated accounts are not affected."
        actions={[{ title: 'Sign out', destructive: true, action: () => { void signOutDefault() } }]}
        onClose={() => setConfirmSignOut(false)}
      />
    </VStack>
  )
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : 'Try again.'
}
