import { Dialog } from '../components/Overlay'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useAccount } from '../store/account'
import { useApp } from '../store/context'

/** Signed in here with another account than the one this computer is in: move it over, or keep it there (`moveOffer`). */
export function MoveComputerPrompt() {
  const tr = useTr()
  const app = useApp()
  useAccount()
  useModel(app.cloud)
  useModel(app.local)
  const offer = app.cloud.moveOffer
  const where = offer?.from ? tr("It's in {name}'s account.").replaceAll('{name}', offer.from) : tr("It's in another account.")
  return (
    <Dialog
      open={offer !== null}
      title={tr('Move this computer to your account?')}
      message={`${where} ${tr('Devices that were approved through the account lose access. Devices paired with a QR code keep it.')} ${tr('New devices ask for access with a 6-digit code.')}`}
      // A choice either way: "keep it there" is remembered, so it isn't a dismissal.
      cancel={null}
      actions={offer ? [
        { title: 'Keep it there', action: () => app.cloud.keepInOtherAccount(offer.store) },
        { title: 'Move here', action: () => void app.cloud.moveHere(offer.store) },
      ] : []}
      onClose={() => {}}
    />
  )
}
