import { useId, useMemo, useRef, useState } from 'react'
import { Button, IconButton, Spinner } from '../../components/Controls'
import { CustomIcon } from '../../components/CustomIcon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useStore } from '../../store/context'
import { IconEditor } from '../../store/icon-editor'
import { account, useAccount } from '../../store/account'
import { Dialog, usePresence } from '../../components/Overlay'
import { useTr } from '../../lib/use-language'
import './custom-icon-editor.css'
import { SwitchRow } from './parts'

/** Changes are saved immediately, independently of the bot's text settings. */
export function CustomIconEditor({ target }: { target: string }) {
  const store = useStore()
  const editor = useModel(useMemo(() => new IconEditor(store, target), [store, target]))
  const session = useAccount()
  const [needsSignIn, setNeedsSignIn] = useState(false)
  const requireAccount = (action: () => void) => { if (session.isSignedIn) action(); else setNeedsSignIn(true) }
  const [url, setURL] = useState('')
  const [showURL, setShowURL] = useState(false)
  const reveal = usePresence(showURL)
  const urlId = useId()
  const tr = useTr()
  const input = useRef<HTMLInputElement>(null)
  const icon = store.icons[target]
  const disabled = editor.busy || session.isBusy || !session.isReady || store.connection.kind !== 'online'
  if (!store.hello?.icons) return null
  return <VStack gap={10} className="custom-icon-editor">
    <Text size="subheadline" weight="semibold">Custom icon</Text>
    <HStack gap={8} wrap justify="center">
      <CustomIcon icon={icon} size={40} fallback={null} />
      <Button kind="secondary" disabled={disabled} onClick={() => requireAccount(() => input.current?.click())}>Upload from desktop</Button>
      <Button kind="secondary" disabled={disabled} onClick={() => requireAccount(() => setShowURL((shown) => !shown))}>Paste URL</Button>
      {icon && <Button disabled={disabled} onClick={() => void editor.save({ remove: true })}>Use default</Button>}
      {editor.busy && <Spinner />}
    </HStack>
    <input ref={input} type="file" hidden accept="image/png,image/jpeg,image/webp,image/svg+xml"
      onChange={(event) => {
        const file = event.currentTarget.files?.[0]
        event.currentTarget.value = ''
        if (file) requireAccount(() => { void editor.save({ file }) })
      }} />
    {reveal.mounted && <div className={`custom-icon-url ${reveal.shown ? 'shown' : ''}`}
      onKeyDown={(event) => { if (event.key === 'Enter' && event.target instanceof HTMLInputElement && !event.nativeEvent.isComposing) { event.preventDefault(); if (!disabled && url.trim()) void editor.save({ url: url.trim() }) } }}>
      <label htmlFor={urlId}>{tr('Image URL')}</label>
      <HStack gap={8}>
        <input id={urlId} className="field-box" type="url" placeholder="https://…" value={url} disabled={disabled}
          autoFocus onChange={(event) => setURL(event.target.value)} />
        <IconButton title="Use URL" icon="arrow.up" size={36} disabled={disabled || !url.trim()}
          onClick={() => void editor.save({ url: url.trim() })} />
      </HStack>
    </div>}
    {icon && <SwitchRow title="Tint icon" detail="Turn off to keep the image's original colors." on={icon.tinted}
      onChange={(tinted) => { if (!disabled) void editor.save({ tinted }) }} />}
    <VStack gap={2} className="custom-icon-help">
      <Text size="caption" tone="secondary">PNG, JPEG, WebP up to 5 MiB; SVG up to 256 KiB.</Text>
      <Text size="caption" tone="secondary">Saved immediately. Custom images stay still.</Text>
    </VStack>
    <Dialog open={needsSignIn} title="Sign in required" message="Sign in to use custom images."
      onClose={() => setNeedsSignIn(false)} actions={[
        { title: 'Continue with Apple', action: () => { setNeedsSignIn(false); void account.signIn('apple') } },
        { title: 'Continue with Google', action: () => { setNeedsSignIn(false); void account.signIn('google') } },
      ]} />
    {(editor.error || session.errorMessage) && <Text size="caption" tone="danger">{editor.error || session.errorMessage}</Text>}
  </VStack>
}
