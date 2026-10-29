import { useCallback, useEffect, useState } from 'react'
import type { AuthorizedDevice } from '@shared/models'
import { CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog } from '../../components/Overlay'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import type { BotStore } from '../../store/bot-store'
import { deviceIcon, errorText, liveClient, relativeDay } from './parts'
import { useTr } from '../../lib/use-language'


/** The devices that can use a computer this app manages, each one revocable. */
export function AuthorizedDevicesSection({ store }: { store: BotStore }) {
  const tr = useTr()
  useModel(store)
  const [devices, setDevices] = useState<AuthorizedDevice[] | { error: string } | null>(null)
  const [confirmRevoke, setConfirmRevoke] = useState<AuthorizedDevice | null>(null)
  const online = store.connection.kind === 'online'
  const name = store.hostName

  const loadDevices = useCallback(async () => {
    const client = liveClient(store)
    if (!client) return
    try {
      setDevices((await client.call<{ devices: AuthorizedDevice[] }>('devices')).devices)
    } catch (e) {
      setDevices({ error: errorText(e) })
    }
  }, [store])

  // Also when it changes account: the devices approved through the old one lose access.
  useEffect(() => void loadDevices(), [loadDevices, online, store.accessRequests.length, store.cloud?.owner?.userId])

  const revoke = async (device: AuthorizedDevice) => {
    try {
      await (await store.ready()).call('revokeDevice', { key: device.key })
    } catch (e) {
      store.setError(errorText(e))
    }
    await loadDevices()
  }

  return (
    <>
      <CardSection title={tr('{name} can be used by').replaceAll('{name}', name)}>
        {devices === null
          ? [<Spinner key="loading" />]
          : 'error' in devices
            ? [<Text key="error" tone="danger" className="settings-appear">{devices.error}</Text>]
            : [
                ...(devices.length === 0
                  ? [<Text key="none" tone="secondary">{tr(`None yet. Pair an iPhone, or approve one from your account.`)}</Text>]
                  : []),
                ...devices.map((device) => (
                  <HStack key={device.key} gap={8} className="settings-row settings-appear">
                    <IconSlot width={18} tone="secondary">
                      <Icon name={deviceIcon(device.platform)} size={13} />
                    </IconSlot>
                    <VStack grow className="settings-stack">
                      <span>{device.name}</span>
                      <Text size="caption" tone="tertiary">{deviceDetail(device, tr)}</Text>
                    </VStack>
                    <IconButton title="Revoke access" icon="xmark.circle" onClick={() => setConfirmRevoke(device)} />
                  </HStack>
                )),
              ]}
      </CardSection>
      <Dialog
        open={confirmRevoke !== null}
        title={tr('Revoke {name}?').replaceAll('{name}', confirmRevoke?.name ?? 'device')}
        message={tr('It disconnects right away and has to pair or ask again.')}
        actions={confirmRevoke ? [{ title: 'Revoke access', destructive: true, action: () => void revoke(confirmRevoke) }] : []}
        onClose={() => setConfirmRevoke(null)}
      />
    </>
  )
}

function deviceDetail(device: AuthorizedDevice, tr: (text: string) => string) {
  const paired = device.source === 'account' ? 'Approved through the account' : 'Paired with a QR code'
  if (device.connected) return tr(`${paired} · connected`)
  if (!device.lastSeenAt) return tr(paired)
  const when = relativeDay(new Date(device.lastSeenAt))
  return tr(`${paired} · seen {when}`).replaceAll('{when}', when === 'Yesterday' ? tr('Yesterday') : when)
}
