import type { Usage, UsageService } from '@shared/models'

/** The agents in `order`, a move still on its way to the host; any it doesn't name follow in the host's order. */
export function servicesIn({ services = [] }: Pick<Usage, 'services'>, order: string[] | null): UsageService[] {
  if (!order) return services
  return [...order.flatMap((id) => services.find((s) => s.id === id) ?? []), ...services.filter((s) => !order.includes(s.id))]
}

/** On, with nothing to show: signed out, or nothing to report. */
export const hasNoReading = ({ providers }: Pick<Usage, 'providers'>, service: UsageService) =>
  service.shown && !providers.some((p) => p.id === service.id)

/** Whether a row dragged over `box` takes its place: past the middle, not the edge, because rows slide as they swap and an edge would swap them straight back. */
export function pastMiddle(down: boolean, y: number, box: { top: number; height: number }) {
  const middle = box.top + box.height / 2
  return down ? y >= middle : y <= middle
}
