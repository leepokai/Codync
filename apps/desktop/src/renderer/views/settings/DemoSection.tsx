import { Button, CardSection } from '../../components/Controls'
import { useModel } from '../../lib/observable'
import { demoMode } from '../../store/demo-mode'

/** Settings → Try demo mode: sample bots on an in-memory host, to look around without touching anything real. */
export function DemoSection() {
  const demo = useModel(demoMode)
  return <CardSection title="Demo mode" footer="Look around with sample bots, a session, an approval and a routine. Nothing is saved or sent, and leaving brings back exactly what you had.">
    {demo.active
      ? <Button kind="secondary" onClick={() => demoMode.exit()}>Exit demo mode</Button>
      : <Button kind="secondary" onClick={() => demoMode.enter()}>Try demo mode</Button>}
  </CardSection>
}
