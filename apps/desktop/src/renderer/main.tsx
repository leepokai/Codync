import { createRoot } from 'react-dom/client'
import './styles/theme.css'
import { ChatWindow } from './views/ChatWindow'
import { PairingWindow } from './views/PairingWindow'
import { AppModel } from './store/app-model'
import { AppContext } from './store/context'
import { applyTextSize, prefs } from './lib/prefs'
import { account } from './store/account'

const page = new URLSearchParams(location.search).get('window') ?? 'chat'
document.title = window.codync.appName
document.documentElement.dataset.platform = window.codync.platform
applyTextSize(prefs.textSize.get())

// The pairing window only needs this computer's host, not a mirror of its bots.
const app = new AppModel(page !== 'pairing')
void app.start()
if (page !== 'pairing') void account.start()

createRoot(document.getElementById('root')!).render(
  <AppContext.Provider value={app}>{page === 'pairing' ? <PairingWindow /> : <ChatWindow />}</AppContext.Provider>,
)
