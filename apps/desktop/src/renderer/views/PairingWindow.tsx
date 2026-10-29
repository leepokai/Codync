import { useCallback, useEffect, useState } from 'react'
import type { CloudStatus, PairingInfo } from '@shared/models'
import { LoopbackTransport } from '../client/host-client'
import { useApp } from '../store/context'
import { PairingCode } from './PairingCode'

/** The pairing window: this computer's code, without a mirror of its bots. */
export function PairingWindow() {
  // By value: the host's snapshot is a new object on each of its changes (every few seconds), and
  // each request issues a new code.
  const { baseURL, token } = useApp().host.local ?? {}
  const [cloudOn, setCloudOn] = useState(false)
  const [name, setName] = useState('this computer')
  // The window stays open between uses: each "Pair iPhone…" shows it again, with a new code.
  const [shown, setShown] = useState(0)
  useEffect(() => window.codync.app.onCommand((c) => {
    if (c.kind === 'reviewApprovals') setShown((n) => n + 1)
  }), [])

  const requestCode = useCallback(async () => {
    if (!baseURL || !token) throw new Error("This computer's host isn't connected.")
    const transport = new LoopbackTransport(baseURL, token)
    const [pairing, status, hello] = await Promise.all([
      transport.call('pairing', {}, 20_000) as Promise<PairingInfo>,
      transport.call('cloudStatus', {}, 20_000).catch(() => null) as Promise<CloudStatus | null>,
      transport.call('hello', {}, 20_000).catch(() => null) as Promise<{ name?: string } | null>,
    ])
    setCloudOn(status?.enabled === true)
    if (hello?.name) setName(hello.name)
    return pairing
  }, [baseURL, token])

  return <PairingCode name={name} cloudOn={cloudOn} requestCode={requestCode} renew={shown} />
}
