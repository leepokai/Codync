import { IconButton, Reveal } from '../components/Controls'
import { Icon } from '../components/Icon'
import { IconSlot, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { prefs, usePref } from '../lib/prefs'
import { useTr } from '../lib/use-language'

/**
 * Once, at the top of the roster with several computers: Extensions left the sidebar footer for
 * each computer's heading. Gone for good when dismissed or once a heading opens Extensions.
 */
export function ExtensionsMovedTip() {
  const tr = useTr()
  const [seen, setSeen] = usePref(prefs.extensionsMovedTipSeen)
  return (
    <Reveal show={!seen}>
      <div className="extensions-tip">
        <IconSlot width={18} tone="secondary" className="extensions-tip-icon">
          <Icon name="square.grid.2x2" size={13} />
        </IconSlot>
        <VStack grow className="extensions-tip-text">
          <Text size="callout" weight="medium">{tr('Extensions moved to each computer')}</Text>
          <Text size="caption" tone="secondary">{tr("Each computer has its own. Open them with the button on the computer's heading.")}</Text>
        </VStack>
        <IconButton title="Dismiss" icon="xmark" size={24} onClick={() => setSeen(true)} />
      </div>
    </Reveal>
  )
}
