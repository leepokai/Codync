/**
 * The roster avatar's lead crown and pause mark, as numbers alone so the layout math is
 * testable without a renderer. AvatarWithStatus draws them; avatar.css positions them.
 *
 * The roster icon follows the user's Appearance size (24 to 44, prefs.rosterIconSize), so every
 * mark on the avatar has to follow the icon too.
 *
 * Before this, a lead bot's crown was floored at 12px while the pause mark was shrunk to fit the
 * gap the crown left: `size - (size/2 + crown/2) - 2`. At the smallest icon that gap went under
 * 8px and the mark was skipped entirely, so a lead bot that was ALSO paused stopped saying
 * Paused on its face at all - the only remaining cue was the small mark beside its name, which
 * read as "the pause moved away from the icon".
 *
 * The mark is no longer shrunk. It keeps its own size and moves down, clear of the crown instead.
 */

/** The lead crown's width for a roster icon of `size`. It grows with the icon, and never falls
 *  below the 12px it used to be floored at, so no icon size wears a smaller crown than before. */
export function crownSize(size: number) {
  return Math.max(12, size * 0.42)
}

/** A paused bot's mark's width: the icon alone decides it, never the crown, so a lead bot that
 *  is also paused wears the same readable mark as any other paused bot. */
export function pauseMarkSize(size: number) {
  return Math.max(9, size * 0.3)
}

/** How far down the pause mark sits, so a crown straddling the frame's top edge cannot cover it.
 *  The crown's box hangs 44% of its width above that edge (avatar.css), so about 56% of its width
 *  reaches below it; the mark starts at 0.62 of the crown's width - just past that overhang, and
 *  still inside the frame at the smallest icon. Zero without a crown. */
export function pauseMarkTop(size: number, lead: boolean) {
  return lead ? crownSize(size) * 0.62 : 0
}
