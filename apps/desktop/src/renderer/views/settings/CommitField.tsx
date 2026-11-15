import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'

/**
 * A settings row whose field saves when it loses focus; Enter blurs it. Uncontrolled, so a host
 * status event never overwrites what is being typed.
 */
export function CommitField(props: { label: string; placeholder: string; defaultValue: string; password?: boolean; disabled: boolean; commit: (input: HTMLInputElement) => void }) {
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

/**
 * A `CommitField` over a value the computer keeps (`saved`): a newly saved one resets the field, a refused one
 * (`save` answers false) puts the saved one back, and an unchanged one isn't sent.
 */
export function SavedField({ label, placeholder, saved, disabled, save }: { label: string; placeholder: string; saved: string; disabled: boolean; save: (typed: string) => Promise<boolean> }) {
  return (
    <CommitField
      key={saved}
      label={label}
      placeholder={placeholder}
      defaultValue={saved}
      disabled={disabled}
      commit={(input) => {
        if (input.value.trim() === saved) return
        void save(input.value).then((kept) => {
          if (!kept) input.value = saved
        })
      }}
    />
  )
}
