import { useCallback, useEffect, useState, type CSSProperties, type ReactNode } from 'react'
import { CharacterAvatar, type Mood } from '../components/Avatar'
import { Button, Spinner } from '../components/Controls'
import { Icon } from '../components/Icon'
import { HStack, Spacer, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useReduceMotion } from '../lib/theme'
import { useTr } from '../lib/use-language'
import { showsWelcome } from '../lib/welcome-gate'
import { prefs, usePref } from '../lib/prefs'
import { account, useAccount } from '../store/account'
import { analytics } from '../store/analytics'
import googleLogo from '../assets/google.svg'
import type { SignInProvider } from '@shared/analytics'
import './account-welcome.css'

const deviceName = window.codync.platform === 'darwin' ? 'Mac' : 'computer'

/** Whether the welcome covers the window (`showsWelcome`). `finish`: the first run's choice was made. */
export function useWelcomeGate() {
  const [onboarded, setOnboarded] = usePref(prefs.onboardingDone)
  const { isConfigured, isReady, isSignedIn } = useAccount()
  const finish = useCallback(() => {
    if (prefs.onboardingDone.get()) return
    setOnboarded(true)
    analytics.track({ event: 'onboarding_completed', signed_in: account.isSignedIn })
  }, [setOnboarded])
  return { shown: showsWelcome({ onboarded, configured: isConfigured, ready: isReady, signedIn: isSignedIn }), finish }
}

/** The welcome: signing in, or, in a build without accounts, going on without one. A restored session skips it as soon as it is back. */
export function AccountWelcomeView({ onContinue }: { onContinue: () => void }) {
  const account = useAccount()
  const reduce = useReduceMotion()
  const tr = useTr()
  const onThis = tr(deviceName === 'Mac' ? 'Continue on this Mac' : 'Continue on this computer')
  const [beat, setBeat] = useState(reduce ? 4 : 0)

  useEffect(() => {
    if (account.isSignedIn) onContinue()
  }, [account.isSignedIn, onContinue])

  useEffect(() => {
    if (reduce) return setBeat(4)
    const timers = [150, 430, 710, 990].map((at, i) => setTimeout(() => setBeat(i + 1), at))
    return () => timers.forEach(clearTimeout)
  }, [reduce])

  if (!account.isReady) {
    // Keep first-run sign-in choices behind the persisted-session restore on launch.
    return (
      <VStack align="center" justify="center" gap={14} className="welcome-restoring">
        <Spinner size={24} />
        <Text as="div" size="callout" tone="secondary">{tr(`Checking for a saved account…`)}</Text>
        {account.isConfigured ? null : (
          <button className="welcome-skip" onClick={onContinue}>
            {onThis}
          </button>
        )}
      </VStack>
    )
  }

  const rise = (shown: boolean): CSSProperties => ({
    opacity: shown ? 1 : 0,
    transform: shown ? 'none' : 'translateY(14px)',
    transition: 'opacity 0.6s var(--ease-out), transform 0.6s var(--ease-out)',
  })

  return (
    <div className="welcome">
      <VStack className="welcome-column">
        <Spacer min={24} />
        <WelcomeCrew shown={beat >= 1} />
        <Spacer min={24} />
        <div className="welcome-glimpse">
          <WelcomeChatGlimpse shown={beat >= 2} />
        </div>
        <VStack gap={10} style={rise(beat >= 3)}>
          <div className="welcome-title">{tr('Your coding agents,\nas teammates.')}</div>
          <Text as="div" size="body" tone="secondary">{tr(`Give each one a name and a project. They work on your computer while you're away.`)}</Text>
        </VStack>
        <Spacer min={32} />
        <VStack gap={10} style={rise(beat >= 4)}>
          {account.isConfigured ? null : (
            <Button onClick={onContinue} className="welcome-button">
              {onThis}
            </Button>
          )}
          <ProviderButton provider="apple" mark={<Icon name="apple.logo" size={20} />} title="Continue with Apple" />
          <ProviderButton provider="google" kind="secondary" mark={<img src={googleLogo} alt="" width={20} height={20} />} title="Continue with Google" />
          <ProviderButton provider="github" kind="secondary" mark={<Icon name="github.logo" size={20} />} title="Continue with GitHub" />
          {!account.isConfigured ? (
            <Text as="div" size="footnote" tone="tertiary">{tr(deviceName === 'Mac' ? "Sign-in isn't configured in this build. You can still use Codync on this Mac." : "Sign-in isn't configured in this build. You can still use Codync on this computer.")}</Text>
          ) : null}
          {account.errorMessage ? <Text as="div" size="footnote" tone="attention">{account.errorMessage}</Text> : null}
        </VStack>
      </VStack>
    </div>
  )
}

/** One provider's sign-in button; every one shares the busy spinner and the disabled rule. */
function ProviderButton({ provider, kind, mark, title }: { provider: SignInProvider; kind?: 'primary' | 'secondary'; mark: ReactNode; title: string }) {
  const account = useAccount()
  const tr = useTr()
  return (
    <Button kind={kind} onClick={() => void account.signIn(provider)} disabled={!account.isConfigured || account.isBusy} className="welcome-button">
      {account.isBusy ? (
        <Spinner size={18} />
      ) : (
        <HStack as="span" gap={10}>
          {mark}
          {tr(title)}
        </HStack>
      )}
    </Button>
  )
}

const crew: { shape: string; color: string; size: number; x: number; y: number; mood: Mood }[] = [
  { shape: 'blob', color: 'blue', size: 92, x: 0, y: 0, mood: 'working' },
  { shape: 'squircle', color: 'orange', size: 64, x: -104, y: -34, mood: 'idle' },
  { shape: 'teardrop', color: 'violet', size: 58, x: 100, y: -46, mood: 'working' },
  { shape: 'hex', color: 'green', size: 48, x: -76, y: 64, mood: 'idle' },
  { shape: 'cloud', color: 'magenta', size: 52, x: 84, y: 58, mood: 'needsInput' },
]

/** The animated bot group from the first iPhone onboarding screen. */
function WelcomeCrew({ shown }: { shown: boolean }) {
  const reduce = useReduceMotion()
  const [t, setT] = useState(0)
  useEffect(() => {
    if (reduce || !shown) return
    let frame = 0
    const tick = (now: number) => {
      setT(now / 1000)
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [reduce, shown])
  return (
    <div aria-hidden className="welcome-crew">
      {crew.map((m, i) => (
        <div
          key={m.shape}
          className="welcome-crew-member"
          style={{
            transform: `translate(calc(-50% + ${m.x}px), calc(-50% + ${m.y + (reduce ? 0 : Math.sin(t * 1.3 + i * 1.7) * 5)}px)) scale(${shown ? 1 : 0.3})`,
            opacity: shown ? 1 : 0,
            transition: `opacity 0.7s var(--ease-out) ${i * 0.07}s, scale 0.7s var(--ease-out) ${i * 0.07}s`,
          }}
        >
          <CharacterAvatar shape={m.shape} color={m.color} size={m.size} mood={m.mood} />
        </div>
      ))}
    </div>
  )
}

/** A short animated user and bot exchange, matching the iPhone onboarding preview. */
function WelcomeChatGlimpse({ shown }: { shown: boolean }) {
  const tr = useTr()
  const reduce = useReduceMotion()
  const [replied, setReplied] = useState(false)
  useEffect(() => {
    if (!shown) return
    const timer = setTimeout(() => setReplied(true), reduce ? 0 : 1600)
    return () => clearTimeout(timer)
  }, [shown, reduce])
  const rise: CSSProperties = { opacity: shown ? 1 : 0, transform: shown ? 'none' : 'translateY(14px)', transition: 'opacity 0.6s, transform 0.6s var(--ease-out)' }
  return (
    <VStack gap={10}>
      <HStack align="stretch" justify="flex-end" style={rise}>
        <Text size="subheadline" tone="text" className="welcome-bubble user">{tr(`Fix the flaky login test`)}</Text>
      </HStack>
      <HStack align="flex-end" gap={8} style={rise}>
        <CharacterAvatar shape="blob" color="blue" size={28} mood={replied ? 'idle' : 'working'} />
        {replied ? (
          <Text size="subheadline" tone="text" className="welcome-bubble agent">
            {tr(`Done. It raced the session refresh; tests pass on fix/login.`)}
          </Text>
        ) : (
          <Text size="subheadline" tone="tertiary">{tr(`Working…`)}</Text>
        )}
      </HStack>
    </VStack>
  )
}
