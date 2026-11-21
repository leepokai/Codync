# Visual language

How every Codync screen looks, on every client: quiet, black and white, the content first and the chrome out of the
way, in the manner of the ChatGPT desktop and iPhone apps (references in [`reference/`](reference/)). The tokens live in
`apps/desktop/src/renderer/styles/theme.css`, CodyncKit's `Palette` / `Design/` (iPhone) and the Android `design`
module; this page says how to use them. Placement rules (which control, which menu) are in
[UI conventions](ui-conventions.md).

## Five rules

1. **One ink, three strengths.** Text is `text`, `secondary` or `tertiary`; nothing else carries meaning by color.
2. **Fill steps, not lines.** A surface is set off by a fill a step from what is behind it: the page, the bubble
   (`bubble-agent`), then tints of the ink over it (`group` 4 %, `field` 6 %, hover/pressed about 9 %). Fill **or**
   border, never both; only floating layers (sheets, dialogs, menus, the message box) get the 1px `outline`.
3. **Three levels of type per block.** A card or row uses at most a headline, the body and a caption.
4. **One radius scale, one spacing grid.** No value outside the tokens below.
5. **Every element has a job.** If its job can't be said in one sentence, or the same fact is already shown elsewhere
   on screen, it goes ([no unexplained elements](#review)).

## Color

| Use | Token |
|---|---|
| What is read: titles, answers, messages | `text` |
| Supporting words: a request quoted, a hint, an inactive control | `secondary` |
| Metadata: where, when, counts | `tertiary` |
| The one thing that must be done next | `accent-fill` (black / white), at most one per screen, never inside a card |
| Needs you | `warning` / `attention` (amber): the status mark only, never a fill or a whole line |
| Error, destructive | `danger` (red): error text and the destructive confirm button only |
| Identity | the bot's own color, on its avatar only |

No green or blue in UI chrome (diff `added`/`removed` excepted), no gradients, no status dots beside every row, no
tinted backgrounds for emphasis. Both themes are designed, not inverted: each token has its dark value in the same file.

## Type

The ramp is the platform's (macOS / iOS text styles; desktop `--text-*`, scaled by the user's text size):

| Role | Style | Weight |
|---|---|---|
| Page title | `title` / `largeTitle` | semibold |
| A card's question, a section's heading | `headline` | semibold |
| Messages, answers, row titles | `body` | regular; medium only for the chosen or first answer |
| Secondary line under a row, quoted request | `subheadline` / `callout` | regular |
| Metadata, hints, timestamps | `caption` | regular, `tertiary` |

- Weight and ink do the hierarchy; size steps stay small. Never bold body text for emphasis inside UI.
- **Monospace only for literal machine text** (a command, a path, a tool name, a key), in `secondary`; when it is a
  quote of what is asked, it sits in a well (`field`, `radius-s`, 8 × 10 padding).
- Sentence case everywhere; no uppercase labels with tracking, no trailing periods on labels.
- Digits that line up (counts, times, numbered answers) are tabular.

## Spacing

A 4-point grid: **2, 4, 8, 12, 16, 24, 32**.

- Inside a group (a title and its caption, a question and its context): 2–4.
- Between groups in a card (the question block, the answers): 12.
- Card padding: 14 on the desktop, 16 on the phones; rows run 8 wider than the card's text so their badges line up
  under it.
- Between cards in a conversation: the transcript's own gap; never extra margins on a card.

## Shape

`radius-xs` 6 (badges, keys) · `radius-s` 10 (rows, wells, fields) · `radius-m` 12 · `radius-l` 16 (a settled, folded
card) · `radius-xl` 22 (cards, bubbles) · pill (chips, the message box). A shape nested in another uses the smaller
step. Shadows only on what floats (`shadow-s` … `shadow-xl`); nothing in the page casts one.

## The card

Every card the agent puts in a conversation (a choice, an approval, a sign-in, a file to review) has one anatomy, so
two cards next to each other read as one family:

1. **Question**: `headline`, what is asked, in words a person uses.
2. **Context line**: `caption` `tertiary`, where and from what (a computer, a folder, a background task); its one
   action (**Details**, **Show more**) sits at the end of this line, `caption` medium `secondary` with a chevron.
3. **Quoted material**, when there is any: the request, the command, the option's preview, in a well.
4. **Answers**: one row per answer, 36 px tall, a 22 px badge (`field` fill, its number or key in `secondary`
   semibold) and the label in `body` `text`; the row fills with `field` on hover or press. No filled buttons, no red,
   the first answer the safe one. The terminal draws the same rows with its own keys.
5. **Settled**: the card folds to one line, `radius-l`: the question's subject and how it ended, in `secondary`.

## Controls

- Icon-only wherever an icon is unambiguous, with a tooltip / accessibility label; words only for answers and choices.
- Hover and press are fills (`field`), never outlines or color shifts; disabled is 40 % opacity.
- Every tap that shows or hides something animates with `Motion` (`layout` for size, `fade` for appearance); nothing
  moves on its own (no pulses, shimmers or loops beyond a spinner while waiting).

## Review

Before a UI change is done, for each new or changed element:

- What is its job, in one sentence? Where else is the same fact shown? Remove it if the answer is "nowhere new".
- Does it use only the tokens above (ink, fill steps, type roles, spacing, radius)?
- Is it the card anatomy, a row, or an existing control, rather than a new shape?
- Does it look right in dark mode and at the largest text size?
- Is it the same on the desktop, iPhone, Android and terminal ([cross-platform rule](../../CLAUDE.md))?
