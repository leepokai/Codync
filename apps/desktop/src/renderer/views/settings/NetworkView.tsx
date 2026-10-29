import { VStack } from '../../components/Stack'
import { useApp } from '../../store/context'
import { NetworkSection } from './NetworkSection'

/** Settings → Network: the networks of each computer this app manages, this one first (a section stays hidden on a host without them). */
export function NetworkView() {
  const managed = useApp().managedStores
  return (
    <div className="settings-computers">
      <VStack gap={26} className="settings-computers-content">
        {managed.map((store) => (
          <NetworkSection key={store.computer.id} store={store} heading={managed.length > 1 ? store.hostName : undefined} />
        ))}
      </VStack>
    </div>
  )
}
