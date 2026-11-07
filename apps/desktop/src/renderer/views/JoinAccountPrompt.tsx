import { Dialog } from '../components/Overlay'
import { withJoinRemoves } from '../lib/account-copy'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useAccount } from '../store/account'
import { useApp } from '../store/context'
import { useJoinRemoves } from './use-join-removes'

/**
 * Signed in, with a computer in no account: add it, or keep it out (docs/plans/optional-sign-in.md). Asked once for this
 * computer (`joinOffer`), and whenever the user asks to add one.
 */
export function JoinAccountPrompt() {
  const tr = useTr()
  const app = useApp()
  useAccount()
  useModel(app.cloud)
  useModel(app.local)
  const store = app.cloud.joinOffer
  const names = useJoinRemoves(store, store !== null)
  // An SSH computer: the Mac in front of the user isn't the one this adds.
  const title = store && store !== app.local ? tr('Add {name} to your account?').replaceAll('{name}', () => store.hostName) : tr('Add this computer to your account?')
  return (
    <Dialog
      open={store !== null}
      title={title}
      message={withJoinRemoves(tr, tr('Without it, phones can connect only over Wi-Fi or Tailscale.'), names)}
      // A choice either way: "not now" is remembered, so it isn't a dismissal.
      cancel={null}
      actions={store ? [
        { title: 'Not now', action: () => app.cloud.declineJoin(store) },
        // Not before it says which phones this removes.
        { title: 'Add to account', disabled: names === undefined, action: () => void app.cloud.acceptJoin(store) },
      ] : []}
      onClose={() => {}}
    />
  )
}
