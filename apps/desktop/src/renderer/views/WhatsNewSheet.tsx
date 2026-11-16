import { BetaTag, CardForm } from '../components/Controls'
import { Icon } from '../components/Icon'
import { ModalHeader, SheetPage } from '../components/Overlay'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useReduceMotion } from '../lib/theme'
import { useTr } from '../lib/use-language'
import { useWhatsNew } from '../lib/use-whats-new'
import type { Feature } from '../lib/whats-new'
import { clipFailed, clipUrl } from '../lib/whats-new-clips'
import { openLink } from '../store/analytics'
import { FormSheet } from './marketplace/pieces'
import './whats-new.css'

/**
 * The highlights of the releases this device hasn't seen, once, over the chat window: each one's clip over its name
 * and a sentence about it, then the smaller changes (docs/features/whats-new.md).
 */
export function WhatsNewSheet({ ready }: { ready: boolean }) {
  const tr = useTr()
  const { open, due, close } = useWhatsNew(ready)
  return (
    <FormSheet open={open} onClose={close}>
      <SheetPage>
        <ModalHeader icon="sparkles" title={tr("What's new")}
          trailing={<Text size="callout" tone="secondary" tabular className="whats-new-version">{due[0]?.version}</Text>} />
        <CardForm>
          {due.map((a) => (
            <VStack key={a.version} gap={24} align="stretch">
              {a.features?.map((feature) => <FeatureStory key={feature.title} feature={feature} />)}
              {a.notes?.length ? (
                <VStack gap={4} align="stretch">
                  <Text as="h3" size="body" weight="semibold" tone="text">{tr('Also in this release')}</Text>
                  {a.notes.map((note) => <Text key={note} size="body" tone="secondary">{tr(note)}</Text>)}
                </VStack>
              ) : null}
            </VStack>
          ))}
        </CardForm>
      </SheetPage>
    </FormSheet>
  )
}

/** One highlight: its looping clip, then its name and what it does. */
function FeatureStory({ feature }: { feature: Feature }) {
  const tr = useTr()
  const still = useReduceMotion()
  const clip = clipUrl(feature.media)
  const { film } = feature
  return (
    <VStack gap={8} align="stretch">
      {clip ? (
        <div className="whats-new-clip">
          {/* A new element when Reduce Motion changes: `autoPlay` alone doesn't stop a clip that's playing. */}
          <video key={still ? 'still' : 'play'} src={clip} autoPlay={!still} muted loop playsInline aria-hidden onError={clipFailed} />
          {film ? (
            <button className="whats-new-film" title={tr('Watch the full video')} aria-label={tr('Watch the full video')} onClick={() => openLink(film, 'whats_new_film')}>
              <Icon name="play.fill" size={11} />
            </button>
          ) : null}
        </div>
      ) : null}
      <HStack gap={7}>
        <Text as="h3" size="title2" weight="semibold" tone="text">{tr(feature.title)}</Text>
        {feature.beta ? <BetaTag /> : null}
      </HStack>
      <Text size="body" tone="secondary">{tr(feature.body)}</Text>
    </VStack>
  )
}
