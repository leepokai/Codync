import { DEMO_SNAPSHOT, DemoTransport } from '../demo/demo-transport'
import { FakeHost } from '../demo/fake-host'
import { macBook } from '../demo/story'
import { Observable } from '../lib/observable'
import { analytics } from './analytics'
import { AppModel } from './app-model'

/**
 * "Try demo mode": while it is on, the window shows a second model on an in-memory host (sample bots, a session with steps
 * and a diff, an approval, a routine) and nothing is read from or written to the device, the network or the real host. The
 * real model keeps running underneath untouched, so leaving brings back exactly what was there.
 */
class DemoMode extends Observable {
  model: AppModel | null = null

  get active() {
    return this.model !== null
  }

  enter() {
    if (this.model) return
    const host = new FakeHost(window.codync.appVersion, macBook)
    const model = new AppModel(true, { transport: () => new DemoTransport(host), snapshot: DEMO_SNAPSHOT })
    // Nothing the person does in the demo is counted.
    analytics.setQuiet(true)
    void model.start()
    this.model = model
    this.changed()
  }

  exit() {
    const model = this.model
    if (!model) return
    this.model = null
    model.dispose()
    analytics.setQuiet(false)
    this.changed()
  }
}

export const demoMode = new DemoMode()
