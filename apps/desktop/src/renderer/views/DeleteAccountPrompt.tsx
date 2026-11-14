import { Dialog } from '../components/Overlay'
import { useTr } from '../lib/use-language'
import { account, useAccount } from '../store/account'
import { useApp } from '../store/context'

/** The account menu's Delete account, confirmed. The computers keep their bots; the cloud forgets the account. */
export function DeleteAccountPrompt() {
  const tr = useTr()
  const app = useApp()
  const { confirmingDelete } = useAccount()
  const remove = async () => {
    if (!(await account.deleteAccount())) app.setError(tr(`Couldn't delete your account. Check your connection and try again.`))
  }
  return (
    <Dialog
      open={confirmingDelete}
      title={tr('Delete your Codync account?')}
      message={tr(`Your account, its list of computers and every device's access through it are deleted. Your computers keep their bots and chats. This can't be undone.`)}
      actions={[{ title: 'Delete account', destructive: true, action: () => void remove() }]}
      onClose={() => account.askToDelete(false)}
    />
  )
}
