import { useEffect, useState, type ReactNode } from 'react'
import type { Backend } from '@shared/models'
import { CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { useStore } from '../../store/context'
import { fill } from '../../lib/fill.ts'
import { useTr } from '../../lib/use-language'
import { errorText } from './market-models'
import { ErrorText } from './pieces'

/** The computer's own login, as the agent's setup knows it: its sign-in state and how to ask again. */
export interface OwnLogin {
  status: string
  checking: boolean
  check: () => void
}

/**
 * An agent's accounts in one place: the default one (the login the computer already has, with its
 * sign-in options as `children`) and the others. Each can be named; others are added (their sign-in
 * follows), signed in or removed. Bots pick theirs in their own settings.
 */
export function AgentAccounts({ backend, own, signIn, signInNow, children }: {
  backend: Backend
  own: OwnLogin
  /** Opens the sign-in terminal for an account. */
  signIn: (account: string) => void
  /** An account whose sign-in was asked for (a chat's sign-in card), started here when its agent signs itself in. */
  signInNow?: string
  children?: ReactNode
}) {
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
  // Its agent signs the account in itself (a browser on the computer), or its CLI does in a terminal.
  const inBrowser = backend.accountSignIn === 'browser'
  const start = (account: string) => (inBrowser ? run(account, () => store.signInAgentAccount(backend.id, account)) : signIn(account))
  const add = () => run('add', async () => start(await store.addAgentAccount(backend.id)))
  const remove = (account: string) => run(account, () => store.removeAgentAccount(account))
  const rename = (account: string | null, name: string) => run(account ?? 'own', () => store.renameAgentAccount(backend.id, account, name))
  useEffect(() => {
    if (signInNow && inBrowser) void start(signInNow)
  }, [])

  const mine = backend.ownAccount
  return (
    <CardSection title="Accounts" footer={`A bot picks its account under Agent in its settings. Bots that pick none use the default account.`}>
      <AccountLine
        name={mine?.name ?? tr('Default account')}
        label={mine?.label ?? null}
        detail={[tr('Default'), mine?.label ? mine.email : null, tr(own.status)]}
        rename={(name) => void rename(null, name)}
      >
        <CheckAgain checking={own.checking} check={own.check} />
      </AccountLine>
      {children}
      {(backend.accounts ?? []).map((account) => (
        <AccountLine
          key={account.id}
          name={account.name ?? tr('New account')}
          label={account.label ?? null}
          detail={[
            account.label ? account.email : null,
            busy === account.id && !account.signedIn && inBrowser
              ? fill(tr('Finish signing in in the browser on {name}…'), { name: store.hostName })
              : tr(account.signedIn ? 'Signed in' : 'Not signed in'),
          ]}
          rename={(name) => void rename(account.id, name)}
        >
          {account.signedIn ? null : <button className="capsule-action agent-step-action" disabled={busy !== null} onClick={() => void start(account.id)}>{tr('Sign in')}</button>}
          {busy === account.id ? <Spinner /> : <IconButton title="Remove account" icon="trash" disabled={busy !== null} onClick={() => void remove(account.id)} />}
        </AccountLine>
      ))}
      <button className="option-row" disabled={busy !== null} onClick={() => void add()}>
        <IconSlot width={26} tone="secondary" aria-hidden>
          <Icon name="plus" size={13} />
        </IconSlot>
        <VStack as="span" gap={2} grow>
          <Text size="body" weight="medium" tone="text">{tr('Add account')}</Text>
          <Text size="subheadline" tone="secondary" lines={3}>{fill(tr(inBrowser ? 'Signs in another {name} account in a browser on {other}.' : 'Signs in another {name} account in a terminal on {other}.'), { name: backend.name, other: store.hostName })}</Text>
        </VStack>
        {busy === 'add' ? <Spinner /> : null}
      </button>
      {error ? <ErrorText text={error} /> : null}
    </CardSection>
  )
}

/**
 * One account: its name, which the pencil turns into a field (Enter saves, Escape leaves it; an empty
 * name goes back to who it signs in), what's known of it, and its actions.
 */
function AccountLine({ name, label, detail, rename, children }: {
  name: string
  label: string | null
  detail: (string | null | undefined)[]
  rename: (name: string) => void
  children?: ReactNode
}) {
  const tr = useTr()
  const [editing, setEditing] = useState(false)
  const save = (typed: string) => {
    setEditing(false)
    if (typed.trim() !== (label ?? '')) rename(typed)
  }
  return (
    <HStack gap={10} className="agent-step-row agent-account">
      <VStack as="span" gap={2} grow>
        {editing ? (
          <input
            className="field-box"
            autoFocus
            aria-label={tr('Account name')}
            placeholder={tr('Account name')}
            defaultValue={label ?? ''}
            maxLength={60}
            onBlur={(e) => save(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') save(e.currentTarget.value)
              if (e.key === 'Escape') setEditing(false)
            }}
          />
        ) : <Text size="body" weight="medium" tone="text" truncate>{name}</Text>}
        <Text size="subheadline" tone="secondary" className="selectable">{detail.filter(Boolean).join(' · ')}</Text>
      </VStack>
      <IconButton title="Rename account" icon="pencil" onClick={() => setEditing(true)} />
      {children}
    </HStack>
  )
}

/** Asks the agent about its sign-in again; an orb while it's asked. */
export function CheckAgain({ checking, check }: { checking: boolean; check: () => void }) {
  const tr = useTr()
  if (checking) return <ThinkingOrb state="connecting" size={16} color="var(--secondary)" />
  return (
    <button title={tr('Check again')} aria-label={tr('Check again')} className="agent-check-again" onClick={check}>
      <Icon name="arrow.clockwise" size={13} />
    </button>
  )
}
