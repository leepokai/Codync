import { DropdownMenu } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { usePref } from '../../lib/prefs'
import { dictationEngine, VOICE_PROVIDERS, voiceInfo } from '../settings/voice-settings'

const ENGINES = [{ id: 'device', title: 'On device' }, ...VOICE_PROVIDERS.map((p) => ({ id: p, title: voiceInfo[p].name }))]

/** Beside the mic, in its capsule: which speech dictation uses, on-device or a provider on the user's own key. */
export function DictationEngineMenu() {
  const [engine, setEngine] = usePref(dictationEngine)
  return (
    <DropdownMenu
      className="press composer-engine"
      title="Dictation engine"
      items={() => ENGINES.map((e) => ({ title: e.title, selected: e.id === engine, action: () => setEngine(e.id) }))}
    >
      <Icon name="chevron.up" size={10} weight="semibold" />
    </DropdownMenu>
  )
}
