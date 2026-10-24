import { useCallback, useEffect, useRef, useState } from 'react'
import QRCode from 'qrcode'
import type { PairingInfo } from '@shared/models'
import { IconButton, Spinner } from '../components/Controls'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useScreenViewed } from '../store/analytics'
import './pairing-code.css'
import { useTr } from '../lib/use-language'


/**
 * A one-time pairing QR (v3) from a computer this app manages: its keys, a code, and how to reach it.
 * A new code comes when `renew` changes and when this one expires on screen; when none comes, the
 * error takes the place of the old code, which may have expired.
 */
export function PairingCode({ name, cloudOn, requestCode, renew = 0 }: { name: string; cloudOn: boolean; requestCode: () => Promise<PairingInfo>; renew?: number }) {
  const tr = useTr()
  const [code, setCode] = useState<{ info: PairingInfo; qr: string } | null>(null)
  const [error, setError] = useState<string | null>(null)
  useScreenViewed('pairing', renew)

  // Loads overlap (expiry, "New code", a renew): only the latest one's code or error shows.
  const latest = useRef(0)
  const load = useCallback(async () => {
    const request = ++latest.current
    try {
      const info = await requestCode()
      const qr = await QRCode.toDataURL(info.pairingUrl, { errorCorrectionLevel: 'M', margin: 0, width: 400, color: { dark: '#000000', light: '#ffffff' } })
      if (request !== latest.current) return
      setCode({ info, qr })
      setError(null)
    } catch (e) {
      if (request !== latest.current) return
      setCode(null)
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [requestCode])

  useEffect(() => void load(), [load, renew])
  const expiresAt = code?.info.expiresAt
  useEffect(() => {
    if (!expiresAt) return
    const timer = setTimeout(() => void load(), Math.max(1000, expiresAt - Date.now()))
    return () => clearTimeout(timer)
  }, [expiresAt, load])

  return (
    <VStack align="center" gap={12} className="pairing">
      {code ? (
        <>
          <div className="pairing-qr fade-in">
            <img src={code.qr} alt={`Pairing code for ${name}`} width={200} height={200} />
          </div>
          <Text as="div" size="caption" tone="secondary" className="pairing-note">
            {tr(`Scan with the Codync app or the iPhone Camera. No Tailscale or open ports needed: your iPhone connects directly on the same Wi-Fi, and through the encrypted relay anywhere else.`)}
          </Text>
          {!cloudOn ? (
            <Text as="div" size="caption" tone="attention" className="pairing-note">
              {code.info.urls.length === 0
                ? tr(`No network address found and “Reach from anywhere” is off. Connect to Wi-Fi or turn it on.`)
                : `“Reach from anywhere” is off, so the iPhone only reaches ${name} on the same network.`}
            </Text>
          ) : null}
          <HStack gap={4}>
            {expiresAt ? (
              <Text size="caption2" tone="tertiary">
                Works once, until {new Date(expiresAt).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}
              </Text>
            ) : null}
            <IconButton title="New code" icon="arrow.clockwise" onClick={() => void load()} />
            <IconButton title="Copy pairing link" icon="doc.on.doc" onClick={() => window.codync.app.copy(code.info.pairingUrl)} />
          </HStack>
        </>
      ) : error ? (
        <>
          <Text as="div" size="caption" tone="danger" className="pairing-note fade-in">{error}</Text>
          <IconButton title="Try again" icon="arrow.clockwise" onClick={() => void load()} />
        </>
      ) : (
        <Spinner size={20} />
      )}
    </VStack>
  )
}
