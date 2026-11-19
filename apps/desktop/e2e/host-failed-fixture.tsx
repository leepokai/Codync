// Loaded into the running window by e2e/host-failed.spec.ts (through the dev server): the app's own
// failed-host screen over the window, for a host state a sandbox's development host never reaches.
import { Observable } from '../src/renderer/lib/observable'
import type { AppModel } from '../src/renderer/store/app-model'
import { AppContext } from '../src/renderer/store/context'
import { HostStateView } from '../src/renderer/views/HostStateView'
import { mountOverlay } from './overlay'

class FailedHost extends Observable {
  constructor(readonly message: string) { super() }
  get host() { return { state: { kind: 'failed' as const, message: this.message } } }
}

export function mount(message: string) {
  return mountOverlay('e2e-host-failed', <AppContext.Provider value={new FailedHost(message) as unknown as AppModel}><HostStateView /></AppContext.Provider>)
}
