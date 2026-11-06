import { DropdownMenu } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { usePref } from '../../lib/prefs'
import { dictationEngine, engineOptions } from '../settings/voice-settings'

/** Beside the mic, in its capsule: which speech dictation uses, on-device or a provider on the user's own key. */
export function DictationEngineMenu() {
  const [engine, setEngine] = usePref(dictationEngine)
  return (
    <DropdownMenu
      className="press composer-engine"
      title="Dictation engine"
      items={() => engineOptions().map((e) => ({ title: e.label, selected: e.id === engine, action: () => setEngine(e.id) }))}
    >
      <Icon name="chevron.up" size={10} weight="semibold" />
    </DropdownMenu>
  )
}
