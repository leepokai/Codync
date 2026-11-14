import { Spinner } from '../components/Controls'
import { usePresence } from '../components/Overlay'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import { useApp } from '../store/context'
import './refresh-indicator.css'

/** Window-wide feedback also stays visible when the sidebar is collapsed. */
export function RefreshIndicator() {
  const app = useApp()
  const tr = useTr()
  const { mounted, shown } = usePresence(app.refreshing, 120)
  if (!mounted) return null
  return (
    <div className={`refresh-indicator ${shown ? 'shown' : ''}`} role="status" aria-live="polite">
      <HStack gap={8}>
        <Spinner size={14} />
        <Text size="callout">{tr('Loading…')}</Text>
      </HStack>
    </div>
  )
}
