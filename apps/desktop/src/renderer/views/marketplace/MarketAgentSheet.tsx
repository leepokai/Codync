import { useEffect, useState } from 'react'
import { CharacterAvatar } from '../../components/Avatar'
import { Button, CardForm, CardSection, Pill, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { errorText, market, CATEGORY_LABELS, type MarketAgent, type MarketAgentDetail, type MarketSkill } from './market-models'
import { ErrorText, ItemRow, MarketSection, SkillGlyph } from './pieces'
import { useTr } from '../../lib/use-language'

/** `Weekdays at 09:00`, `Mondays at 08:49`, `Day 1 of each month at 09:00`. */
export function routineSchedule(schedule: MarketAgentDetail['routines'][number]['schedule']): string {
  const time = schedule.time ?? ''
  switch (schedule.kind) {
    case 'weekdays':
      return `Weekdays at ${time}`
    case 'weekly':
      return `${['Sundays', 'Mondays', 'Tuesdays', 'Wednesdays', 'Thursdays', 'Fridays', 'Saturdays'][schedule.weekday ?? 1]} at ${time}`
    case 'monthly':
      return `Day ${schedule.day ?? 1} of each month at ${time}`
    default:
      return `Daily at ${time}`
  }
}

/**
 * A marketplace agent's page: who made it, its title, the skills it brings (each opens its page), its
 * routines, and its instructions with the category, version and date.
 */
export function MarketAgentSheet({ agent, added, onAdded, onSkill }: { agent: MarketAgent; added: boolean; onAdded: () => void; onSkill: (skill: MarketSkill) => void }) {
  const tr = useTr()
  const store = useStore()
  const [detail, setDetail] = useState<MarketAgentDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const installed = added || agent.installed

  useEffect(() => {
    let live = true
    void store
      .ready()
      .then((c) => market.marketAgent(c, agent.id))
      .then((d) => live && setDetail(d), (e: unknown) => live && setError(errorText(e)))
    return () => {
      live = false
    }
  }, [agent.id])

  const add = async () => {
    setBusy(true)
    setError(null)
    try {
      await store.installAgent(agent.id)
      onAdded()
    } catch (e) {
      setError(errorText(e))
    }
    setBusy(false)
  }

  const copy = () => void navigator.clipboard.writeText(`${window.codync.appScheme}://marketplace/agents/${agent.id}`)
  const open = (s: MarketAgentDetail['skills'][number]) =>
    onSkill({ source: `openbot/${s.id}`, name: s.name, description: s.description, installed: false })

  const trailing = installed ? (
    <HStack gap={6}>
      <Button kind="secondary" onClick={copy}><Icon name="link" size={12} />{tr('Copy link')}</Button>
      <span title={tr('Added')} aria-label={tr('Added')} className="skill-added">
        <Icon name="checkmark" size={13} weight="semibold" />
      </span>
    </HStack>
  ) : busy ? <Spinner /> : (
    <HStack gap={6}>
      <Button kind="secondary" onClick={copy}><Icon name="link" size={12} />{tr('Copy link')}</Button>
      <Button onClick={() => void add()}><Icon name="plus" size={12} />{tr('Add agent')}</Button>
    </HStack>
  )

  return (
    <SheetPage>
      <ModalHeader
        icon={<CharacterAvatar shape={agent.avatarShape} color={agent.avatarColor} size={28} />}
        title={agent.name}
        trailing={trailing}
      />
      <CardForm>
        <CardSection>
          <VStack gap={4}>
            <HStack gap={8} align="center">
              <Text tone="text" weight="medium">{agent.title}</Text>
              <Pill>{tr(installed ? 'Added' : 'Not added')}</Pill>
            </HStack>
            <Text size="caption" tone="secondary">
              {tr('Created by another OpenBot user. OpenBot reviewed it before publishing. Once added, it can act on your behalf.')}
            </Text>
          </VStack>
        </CardSection>
        {detail?.skills.length ? (
          <MarketSection title={tr('Skills')}>
            {detail.skills.map((s) => (
              <ItemRow
                key={s.id}
                title={s.name}
                subtitle={s.description}
                icon={<SkillGlyph />}
                onOpen={() => open(s)}
                trailing={<Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />}
              />
            ))}
          </MarketSection>
        ) : null}
        {detail?.routines.length ? (
          <MarketSection title={tr('Routines')}>
            {detail.routines.map((r) => (
              <ItemRow
                key={r.name}
                title={r.name}
                subtitle={`${routineSchedule(r.schedule)} · ${r.active ? tr('Active') : tr('Inactive')}`}
                icon={<div className="market-tile"><Icon name="calendar.badge.clock" size={14} /></div>}
              />
            ))}
          </MarketSection>
        ) : null}
        <CardSection title={tr('Information')}>
          <VStack gap={10}>
            <Text tone="text" className="selectable">{detail?.instructions ?? agent.title}</Text>
            <dl className="market-facts">
              <dt>{tr('Creator')}</dt><dd>{agent.creator}</dd>
              <dt>{tr('Origin')}</dt><dd>{tr('Reviewed by OpenBot')}</dd>
              <dt>{tr('Category')}</dt><dd>{tr(CATEGORY_LABELS[agent.category] ?? agent.category)}</dd>
              <dt>{tr('Version')}</dt><dd>{agent.version}</dd>
              <dt>{tr('Updated')}</dt><dd>{agent.updated}</dd>
            </dl>
          </VStack>
        </CardSection>
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
        {!detail && !error ? <CardSection><Spinner /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}
