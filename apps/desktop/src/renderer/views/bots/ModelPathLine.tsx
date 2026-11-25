import { Text } from '../../components/Text'
import { useTruncated } from '../../lib/use-truncated'

/**
 * A bot's model path on one muted line, with the whole path in a hover tip when — and only when —
 * the line is clipped. A path that fits says nothing extra: the tip appears because the reader
 * cannot see the end of it, which is how Ryan asked for it ("hover popup the whole model path if it
 * goes to ... due to narrow window").
 *
 * The tip is the app's own tooltip: a `title` attribute, the same one the roster's bot row uses
 * (`hoverTip` in @shared/model-path joins its lines). No bubble component is invented — a popup
 * over a scrolling sheet would be a second tooltip system.
 *
 * `className` is the caller's own layout and indent; the truncation and the tip are this
 * component's, from the one `useTruncated` hook, so a second row that shows a path clips and tips
 * the same way rather than growing its own copy of the measuring logic.
 */
export function ModelPathLine({ path, className }: { path: string; className?: string }) {
  const [ref, truncated] = useTruncated(path)
  return (
    <Text ref={ref} size="caption" tone="secondary" truncate className={className} title={truncated ? path : undefined}>
      {path}
    </Text>
  )
}
