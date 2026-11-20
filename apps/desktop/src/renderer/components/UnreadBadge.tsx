/** An unread count in the accent capsule: a bot's, or a folded section's. `waiting`: it counts a card waiting on the
 * user, so it takes the needs-you colour of the avatar badge (iOS and Android `UnreadBadge`). */
export function UnreadBadge({ count, waiting }: { count: number; waiting: boolean }) {
  return <span className={waiting ? 'unread-badge waiting' : 'unread-badge'}>{count}</span>
}
