# Desktop styles

How the desktop renderer (`apps/desktop/src/renderer/`) styles its views. Views hold no styling of their own: text, layout and visuals come from the shared primitives and tokens below, and an inline `style` is left only for values computed at runtime.

## Tokens (`styles/theme.css`)

- **Color**: `--text`, `--secondary`, `--tertiary`, `--danger`, `--attention` (status text), `--accent`, surfaces (`--background`, `--surface`, `--panel`, `--bubble-*`, `--chat-*`, `--chrome-fill`, `--composer-fill`), strokes (`--border`, `--chrome-stroke`, `--stroke-strong`, `--outline` for the 1px edge of floating layers: the composer, sheets, dialogs and menus), fills with their ink (`--accent-fill`/`--on-accent`, `--danger-fill`/`--on-danger`, `--warning`/`--on-warning`), `--added`/`--removed`, `--code-background`, `--scrim`, the chosen option of a segmented choice or chip (`--chip-on`, `--chip-shadow`). Sheets and dialogs sit on `--sheet` (a step above the page in dark mode) with grouped rows on `--group` and text fields on `--field`, tints that stack; inside `.modal-content` the page tokens point at them (`--background` → `--sheet`, `--surface` and `--bubble-agent` → `--group`, `--bubble-user` → `--field`), so a view styled with the page tokens fits a sheet unchanged. Every color has a light and a dark value. Views use the tokens, or `rgba(var(--text-rgb), a)` for a tint of the text color; a literal only where the color must not follow the theme (white behind a QR code or a favicon).
- **Type ramp**: `--text-large-title` 26, `--text-title` 22, `--text-title2` 17, `--text-title3` 15, `--text-body` 13, `--text-callout` 12, `--text-caption` 11 (points, scaled by the text size preference). `lib/fonts.ts` maps the macOS style names onto it (`headline` and `body` → body, `compactBody` → callout, `subheadline`, `footnote`, `caption`, `caption2`, `compactSecondary` → caption).
- **Radius**: `--radius-xs` 6, `--radius-s` 10, `--radius-m` 12, `--radius-l` 16, `--radius-xl` 22, `--radius-pill`. **Shadow**: `--shadow-s` (resting), `--shadow-m` (floating chrome), `--shadow-l` (menus, panels), `--shadow-xl` (sheets, dialogs).
- **Motion**: `--hover`, `--fade`, `--press`, `--morph`, `--layout`, `--ease`, `--ease-out`, `--spring`. The island has its own (`--island-spring`, `--island-fold`) in `views/island/island.css`.

## Primitives (`components/`)

| Need | Use |
| --- | --- |
| Text in a type style and color | `<Text size="caption" tone="secondary" weight="semibold" mono as="div">` (`Text.tsx`). With `size` the weight defaults to regular (bold for `headline`); without it, size and weight come from around it. `truncate` ends one line with an ellipsis, `lines={n}` clamps to n lines, `grow` takes a stack's free space, `tabular` lines digits up |
| A row / a column | `<HStack gap={8}>` (centered vertically; `align="stretch"` or `"baseline"` otherwise) / `<VStack gap={8}>` (`Stack.tsx`); `grow` takes the free space, `shrink` lets it shrink so text inside can truncate; `justify`, `wrap`, `as` |
| Space between stack items | `<Spacer />`, `<Spacer min={8} />` |
| A fixed-width column that centers an icon, so labels line up | `<IconSlot width={18} tone="secondary">` |
| A row's title with a note under it | `RowLabel` (`Controls.tsx`) |
| Reorder by dragging | `pointerDrag` (`lib/pointer-drag.ts`), not native drag and drop: the pointer holds a closed hand, and a roster row lifts as a smaller, translucent copy that settles into its slot |
| A sheet's content | `<SheetPage>` with a `ModalHeader` (`Overlay.tsx`); `ModalHeader` takes an `icon` (an SF Symbol name on a tile, or a logo element), a title that wraps to two lines, a `subtitle` (up to two lines, the rest in its tooltip), and `onClose` when it heads a panel instead of a modal. A `Sheet` without `height` fits its content up to `maxHeight` (and the window); `FormSheet` does that unless `fixed` (content that fills the card or slides in pages that do: a terminal, an agent sheet) |
| One line with an ellipsis on an element that isn't `Text` | `.truncate` |
| Controls | `Controls.tsx`, `Overlay.tsx`, `Icon.tsx`; `Hairline`, `Pill`, `Switch` and `ProgressBar` take a `className` for a view's own sizing, and a panel beside an anchor is placed with `anchorPlacement` |

## Where a style goes

1. Text styling is `Text` props; layout is `HStack`/`VStack`/`Spacer`.
2. Anything else static (padding, background, radius, size, position, borders) is a class in the view's CSS file, named after the view (`.bot-row`, `.unread-badge`), written with the tokens above.
3. No static inline styles. Inline `style` only for values computed at runtime: a size or color from props or data, a measured width, a transform that follows state. Keep the static part of that element in its class.
4. A pattern repeated across views becomes a primitive or a shared class here; don't copy it.
5. Anything clickable shows the pointing hand (a `:where` rule in `theme.css`, so a view's own `cursor` wins); text fields show the text cursor.
