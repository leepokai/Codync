import { useTr } from '../../lib/use-language'
import { CommitField, SavedField } from './CommitField'

/** The computer's relay address. */
export function RelayAddressRow({ url, disabled, save }: { url: string | null | undefined; disabled: boolean; save: (typed: string) => Promise<boolean> }) {
  return <SavedField label="Relay address" placeholder="https://" saved={url ?? ''} disabled={disabled} save={save} />
}

/** A private relay's registration token. Write-only: the field only says whether one is saved. */
export function RelayTokenRow({ tokenSet, disabled, save }: { tokenSet: boolean; disabled: boolean; save: (token: string) => void }) {
  const tr = useTr()
  return (
    <CommitField
      label="Relay token"
      password
      placeholder={tokenSet ? tr('Saved. Type a new one to replace it.') : tr("The token from the relay's setup")}
      defaultValue=""
      disabled={disabled}
      commit={(input) => {
        const typed = input.value
        input.value = ''
        save(typed)
      }}
    />
  )
}
