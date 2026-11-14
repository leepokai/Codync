import { useEffect, useState, type ReactNode } from 'react'
import type { Backend } from '@shared/models'
import { CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { useStore } from '../../store/context'
import { fill } from '../../lib/fill.ts'
import { keyDetail } from '../../lib/login-profile-keys'
import { useTr } from '../../lib/use-language'
import { ErrorText, OptionRow, useBusyAction } from './pieces'

/** The computer's own login, as the agent's setup knows it: its sign-in state and how to ask again. */
export interface OwnLogin {
  status: string
  checking: boolean
  check: () => void
}

/**
 * An agent's login profiles in one place: the default one (the login the computer already has, with its
 * sign-in options as `children`) and the others, each the agent's own sign-in (an account) or keys (an API
 * key). Each can be named; others are added, signed in or given keys, and removed. Bots pick theirs in their
 * own settings. Any agent takes API keys; an account signs in with the agent's own CLI, so it needs that installed.
 */
export function LoginProfiles({ backend, own, signIn, editKeys, signInNow, children }: {
  backend: Backend
  own: OwnLogin
  /** Opens the sign-in terminal for an account profile. */
  signIn: (loginProfile: string) => void
  /** Opens the key form of an API key profile. */
  editKeys: (loginProfile: string) => void
  /** An account profile whose sign-in was asked for (a chat's sign-in card), started here when its agent signs itself in. */
  signInNow?: string
  children?: ReactNode
}) {
  const store = useStore()
  const tr = useTr()
  const { busy, error, run } = useBusyAction()
  // Its agent signs the account in itself (a browser on the computer), or its CLI does in a terminal.
  const inBrowser = backend.loginProfileSignIn === 'browser'
  const start = (id: string) => (inBrowser ? run(id, () => store.signInLoginProfile(backend.id, id)) : signIn(id))
  const addAccount = () => run('account', async () => start(await store.addLoginProfile(backend.id, 'account')))
  const addKey = () => run('apiKey', async () => editKeys(await store.addLoginProfile(backend.id, 'apiKey')))
  const remove = (id: string) => run(id, () => store.removeLoginProfile(id))
  const rename = (id: string | null, name: string) => run(id ?? 'default', () => store.renameLoginProfile(backend.id, id, name))
  useEffect(() => {
    if (signInNow && inBrowser) void start(signInNow)
  }, [])

  const mine = backend.defaultLoginProfile
  return (
    <CardSection title="Login profiles" footer="A bot picks its login profile under Agent in its settings. Bots that pick none use the default one.">
      <LoginProfileLine
        name={mine?.name ?? tr('Default login profile')}
        label={mine?.label ?? null}
        detail={[tr('Default'), mine?.label ? mine.email : null, tr(own.status)]}
        rename={(name) => void rename(null, name)}
      >
        <CheckAgain checking={own.checking} check={own.check} />
      </LoginProfileLine>
      {children}
      {(backend.loginProfiles ?? []).map((profile) => (
        <LoginProfileLine
          key={profile.id}
          name={profile.name ?? tr('New login profile')}
          label={profile.label ?? null}
          detail={profile.kind === 'apiKey' ? keyDetail(profile).map((part) => tr(part)) : [
            profile.label ? profile.email : null,
            busy === profile.id && !profile.signedIn && inBrowser
              ? fill(tr('Finish signing in in the browser on {name}…'), { name: store.hostName })
              : tr(profile.signedIn ? 'Signed in' : 'Not signed in'),
          ]}
          rename={(name) => void rename(profile.id, name)}
        >
          {profile.kind === 'apiKey'
            ? <button className="capsule-action agent-step-action" disabled={busy !== null} onClick={() => editKeys(profile.id)}>{tr('Edit keys')}</button>
            : profile.signedIn ? null : <button className="capsule-action agent-step-action" disabled={busy !== null} onClick={() => void start(profile.id)}>{tr('Sign in')}</button>}
          {busy === profile.id ? <Spinner /> : <IconButton title="Remove login profile" icon="trash" disabled={busy !== null} onClick={() => void remove(profile.id)} />}
        </LoginProfileLine>
      ))}
      {backend.loginProfileSignIn && backend.installed ? (
        <OptionRow
          icon="plus"
          title={tr('Add account')}
          detail={fill(tr(inBrowser ? 'Signs in another {name} account in a browser on {other}.' : 'Signs in another {name} account in a terminal on {other}.'), { name: backend.name, other: store.hostName })}
          disabled={busy !== null}
          trailing={busy === 'account' ? <Spinner /> : null}
          onClick={() => void addAccount()}
        />
      ) : null}
      <OptionRow
        icon="key"
        title={tr('Add API key')}
        detail={fill(tr('Runs {name} on keys you type or import from your login shell.'), { name: backend.name })}
        disabled={busy !== null}
        trailing={busy === 'apiKey' ? <Spinner /> : null}
        onClick={() => void addKey()}
      />
      {error ? <ErrorText text={error} /> : null}
    </CardSection>
  )
}

/**
 * One login profile: its name, which the pencil turns into a field (Enter saves, Escape leaves it; an empty
 * name goes back to who it signs in), what's known of it, and its actions.
 */
function LoginProfileLine({ name, label, detail, rename, children }: {
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
    <HStack gap={10} className="agent-step-row login-profile">
      <VStack as="span" gap={2} grow>
        {editing ? (
          <input
            className="field-box"
            autoFocus
            aria-label={tr('Login profile name')}
            placeholder={tr('Login profile name')}
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
      <IconButton title="Rename login profile" icon="pencil" onClick={() => setEditing(true)} />
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
