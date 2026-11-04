import { Button } from '../components/Controls'
import { Icon } from '../components/Icon'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import { demoMode } from '../store/demo-mode'
import './demo-badge.css'

/** Always visible while the demo is on: nothing here is real, and one click leaves. */
export function DemoBadge() {
  return (
    <HStack gap={8} className="demo-badge">
      <Icon name="sparkles" size={13} color="var(--background)" />
      <Text size="footnote" tone="text" className="demo-badge-text">Demo mode · sample bots, nothing here is real</Text>
      <Button kind="secondary" onClick={() => demoMode.exit()}>Exit demo</Button>
    </HStack>
  )
}
