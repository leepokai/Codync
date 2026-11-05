import { IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { Notice } from './screen-guidance'
import './screen-viewer.css'

/** A viewer's cover: a centered icon (or a spinner), a line and an optional retry. `compact` fits the preview card. */
export function ScreenNotice({ notice: { icon, text, retry: retryable, dim }, retry, compact = false }: { notice: Notice; retry: () => void; compact?: boolean }) {
  return (
    <VStack gap={compact ? 8 : 12} align="center" className={`screen-notice${dim ? ' dim' : ''}${compact ? ' compact' : ''}`}>
      {icon ? <Icon name={icon} size={compact ? 22 : 36} color="var(--secondary)" /> : <Spinner size={compact ? 18 : 22} />}
      <Text size={compact ? 'caption' : 'callout'} tone="secondary">{text}</Text>
      {retryable ? <IconButton title="Try again" icon="arrow.clockwise" size={compact ? 26 : 32} onClick={retry} /> : null}
    </VStack>
  )
}
