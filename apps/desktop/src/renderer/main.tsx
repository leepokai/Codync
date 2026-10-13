import { createRoot } from 'react-dom/client'
import './styles/theme.css'
import { ChatWindow } from './views/ChatWindow'
import { PairingWindow } from './views/PairingWindow'
import { IslandWindow } from './views/island/IslandWindow'
import { IslandModel } from './views/island/island-model'
import { AppModel } from './store/app-model'
import { AppContext } from './store/context'
import { applyTextSize, prefs } from './lib/prefs'
import { account } from './store/account'

const page = new URLSearchParams(location.search).get('window') ?? 'chat'
document.title = window.codync.appName
document.documentElement.dataset.platform = window.codync.platform
document.documentElement.dataset.window = page
applyTextSize(prefs.textSize.get())
const root = createRoot(document.getElementById('root')!)

if (page === 'island') {
  // The island only draws what the chat window sends it: no host connection of its own.
  const island = new IslandModel()
  island.start()
  root.render(<IslandWindow model={island} />)
} else {
  // The pairing window only needs this computer's host, not a mirror of its bots.
  const app = new AppModel(page !== 'pairing')
  void app.start()
  if (page !== 'pairing') void account.start()
  root.render(<AppContext.Provider value={app}>{page === 'pairing' ? <PairingWindow /> : <ChatWindow />}</AppContext.Provider>)
}
