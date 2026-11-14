import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'

/**
 * A settings row whose field saves when it loses focus; Enter blurs it. Uncontrolled, so a host
 * status event never overwrites what is being typed.
 */
function CommitField(props: { label: string; placeholder: string; defaultValue: string; password?: boolean; disabled: boolean; commit: (input: HTMLInputElement) => void }) {
  const tr = useTr()
  return (
    <div className="settings-row settings-appear">
      <Text tone="text">{tr(props.label)}</Text>
      <input
        className="settings-field"
        aria-label={tr(props.label)}
        type={props.password ? 'password' : undefined}
        autoComplete={props.password ? 'off' : undefined}
        placeholder={props.placeholder}
        defaultValue={props.defaultValue}
        spellCheck={false}
        disabled={props.disabled}
        onBlur={(e) => props.commit(e.currentTarget)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur()
        }}
      />
    </div>
  )
}

/** The computer's relay address. A new saved address resets the field; a refused one puts the saved one back. */
export function RelayAddressRow({ url, disabled, save }: { url: string | null | undefined; disabled: boolean; save: (typed: string) => Promise<boolean> }) {
  const saved = url ?? ''
  return (
    <CommitField
      key={saved}
      label="Relay address"
      placeholder="https://"
      defaultValue={saved}
      disabled={disabled}
      commit={(input) =>
        void save(input.value).then((kept) => {
          if (!kept) input.value = saved
        })
      }
    />
  )
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
