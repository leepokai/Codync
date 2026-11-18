// Loaded into the running window by e2e/update-card.spec.ts (through the dev server): the app's own
// update cards over the window, for a version mismatch and an app floor a sandbox never reaches.
import type { AppLanguage } from '../src/renderer/lib/l10n'
import { Observable } from '../src/renderer/lib/observable'
import { prefs } from '../src/renderer/lib/prefs'
import type { AppModel } from '../src/renderer/store/app-model'
import type { BotStore } from '../src/renderer/store/bot-store'
import { appFloor } from '../src/renderer/store/cloud'
import { AppContext, StoreContext } from '../src/renderer/store/context'
import { AppFloorCard, UpdateNeededCard } from '../src/renderer/views/UpdateNeededCard'
import { mountOverlay } from './overlay'

class FakeStore extends Observable {
  mismatch = { kind: 'updateHost', version: '2.0.0', minimum: '2.50.0' }
  hostName = 'Mini'
  isCloudUpdating = false
  hostUpdateOffer = { kind: 'available' }
  hello = { os: 'macos' }
  requestHostUpdate() {}
}
class FakeApp extends Observable {
  local = {}
  host = { dev: true }
}

export function mount() {
  appFloor.reached('The cloud needs a newer app.')
  return mountOverlay('e2e-update-card', (
    <AppContext.Provider value={new FakeApp() as unknown as AppModel}>
      <StoreContext.Provider value={new FakeStore() as unknown as BotStore}>
        <UpdateNeededCard />
        <AppFloorCard />
      </StoreContext.Provider>
    </AppContext.Provider>
  ), 'flex-direction:column;gap:12px;padding:24px')
}

/** Cards for a computer this app can't update (Linux) and for an update it asked for that didn't finish. */
export function mountHowTo() {
  const store = (fields: Partial<FakeStore>) => Object.assign(new FakeStore(), fields) as unknown as BotStore
  const cards = [
    store({ hostUpdateOffer: { kind: 'unavailable' }, hello: { os: 'linux' } }),
    store({ hostUpdateOffer: { kind: 'available', last: { kind: 'requested', at: 0 } } as FakeStore['hostUpdateOffer'] }),
  ]
  return mountOverlay('e2e-update-card', (
    <AppContext.Provider value={new FakeApp() as unknown as AppModel}>
      {cards.map((s, i) => <StoreContext.Provider key={i} value={s}><UpdateNeededCard /></StoreContext.Provider>)}
    </AppContext.Provider>
  ), 'flex-direction:column;gap:12px;padding:24px')
}

/** Picks a language in Settings while the cards stay on screen. */
export function speak(language: AppLanguage) {
  prefs.appLanguage.set(language)
}
