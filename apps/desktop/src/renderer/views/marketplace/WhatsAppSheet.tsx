import { useEffect, useMemo, useState } from 'react'
import QRCode from 'qrcode'
import { Button, CardForm, CardSection, Spinner } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { useApp } from '../../store/context'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { useModel } from '../../lib/observable'
import { ErrorText } from './pieces'

/**
 * Linking a WhatsApp number to this computer. WhatsApp has no official desktop client API: the
 * number becomes a linked device, and a separate number is safer because WhatsApp can block one it
 * sees as automated.
 */
export function WhatsAppSheet() {
  const tr = useTr()
  const store = useModel(useStore())
  const app = useApp()
  const [qrImage, setQrImage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const state = store.whatsapp
  const selected = state?.bot ?? app.selection?.botId ?? null
  const bots = [...store.bots.values()]

  useEffect(() => {
    void store.refreshWhatsApp()
  }, [store])

  useEffect(() => {
    if (!state?.qr) return setQrImage(null)
    void QRCode.toDataURL(state.qr, { errorCorrectionLevel: 'M', margin: 0, width: 360 }).then(setQrImage, () => setQrImage(null))
  }, [state?.qr])

  // A bot is chosen for the link, not by opening its chat first.
  const chosen = useMemo(() => bots.find((bot) => bot.id === selected), [bots, selected])

  const connect = async () => {
    setError(null)
    try {
      await store.whatsappConnect()
    } catch (e) {
      setError((e as Error).message)
    }
  }

    return (
    <SheetPage>
      <ModalHeader title={tr('WhatsApp')} subtitle={tr('Talk to your bots from a linked WhatsApp number.')} />
      <CardForm>
        <CardSection>
          <VStack gap={10}>
            {state?.state === 'qr' ? (
              <>
                <Text tone="text">{tr('On your phone, open WhatsApp → Linked devices → Link a device, then scan this code.')}</Text>
                {qrImage ? <img className="whatsapp-qr" src={qrImage} width={220} height={220} alt="WhatsApp QR code" /> : <Spinner />}
                <Text size="caption" tone="secondary">
                  {tr('WhatsApp does not support this kind of client officially and can block a number it sees as automated. A separate number is safer.')}
                </Text>
              </>
            ) : state?.state === 'connected' ? (
              <>
                <Text tone="text" weight="medium">{state.name || state.me}</Text>
                <Text size="caption" tone="secondary">{tr('Messages from your allowed chats reach the bot you pick. A workflow can also start from a mention.')}</Text>
                <HStack gap={8}>
                  <Button kind="secondary" onClick={() => void store.whatsappLogout()}>{tr('Unlink')}</Button>
                </HStack>
                <AllowedChats allowed={state.allowed ?? []} onSet={(jid, allowed) => void store.whatsappSetAllowed(jid, allowed)} />
              </>
            ) : state?.state === 'starting' ? (
              <HStack gap={8}><Spinner size={18} /><Text tone="secondary">{tr('Starting the WhatsApp link…')}</Text></HStack>
            ) : (
              <>
                <Text tone="secondary">
                  {tr('Link this computer to a WhatsApp number with a QR code, as another device. Messages arrive here; nothing goes through a Codync server.')}
                </Text>
                <Button onClick={() => void connect()}>{tr('Link WhatsApp')}</Button>
              </>
            )}
            <ValueRow label={tr('Bot')}>
              <select
                className="field-box"
                value={chosen?.id ?? ''}
                onChange={(e) => void store.whatsappSetBot(e.target.value || null)}
              >
                <option value="">{tr('No bot')}</option>
                {bots.map((bot) => <option key={bot.id} value={bot.id}>{bot.name}</option>)}
              </select>
            </ValueRow>
            {state?.error ? <Text size="caption" tone="attention">{state.error}</Text> : null}
            {error ? <ErrorText text={error} /> : null}
          </VStack>
        </CardSection>
      </CardForm>
    </SheetPage>
  )
}

/** A labelled row; the marketplace's inputs share it. */function ValueRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <VStack gap={4}>
      <Text size="caption" tone="secondary">{label}</Text>
      {children}
    </VStack>
  )
}

/** Who may write to the number: the owner always, and these chats and people when the list is not empty. */
function AllowedChats({ allowed, onSet }: { allowed: string[]; onSet: (jid: string, allowed: boolean) => void }) {
  const tr = useTr()
  const [jid, setJid] = useState('')
  const add = () => { const value = jid.trim(); if (value) { onSet(value, true); setJid('') } }
  return (
    <VStack gap={6}>
      <Text size="caption" tone="secondary">
        {allowed.length === 0 ? tr('Anyone can write right now. Add a chat or person to restrict it.') : tr('Allowed chats')}
      </Text>
      {allowed.map((entry) => (
        <HStack key={entry} gap={8}>
          <Text grow size="caption" mono>{entry}</Text>
          <Button kind="secondary" onClick={() => onSet(entry, false)}>{tr('Remove')}</Button>
        </HStack>
      ))}
      <HStack gap={8}>
        <input className="field-box" value={jid} placeholder="5215555555555@s.whatsapp.net" onChange={(e) => setJid(e.target.value)} />
        <Button kind="secondary" disabled={!jid.trim()} onClick={add}>{tr('Allow')}</Button>
      </HStack>
    </VStack>
  )
}
