# macOS chat interaction

The reference is the locally installed Grok Bot 0.66.0 desktop chat. Codync renders
its own messages and actions in SwiftUI; it does not embed Electron or a web view.
The scope is the transcript, bubbles, composer and inspector controls, not the
roster, account pages or Grok-specific services.

## Scrolling

Both Mac transcripts use `MacChatList`: an AppKit `NSScrollView` with a flipped
document and individual SwiftUI hosting controllers for nearby rows. Native
wheel and momentum scrolling move the document without publishing its offset
into SwiftUI. A coalesced bounds notification reconciles the viewport plus one
screen of overscan on each side. Unchanged, mounted rows keep their SwiftUI root
and frame; rows leaving this range release their hosting controllers.
If a large movement would expose an unmounted row, reconciliation runs before
returning from the bounds notification. Incoming views are laid out before they
are revealed, and outgoing views retire within the same non-animated layer
transaction. Individual row hosting views do not inherit window safe-area insets.

`MacChatLayout` keeps measured heights and prefix positions by stable message
ID. Only new/changed mounted rows are measured. Width and typography changes
invalidate measurements; existing heights remain estimates until replacement
measurements arrive. Row-local height changes (for example loaded images) also
update the index. Every geometry correction preserves a message ID and pixel
offset within it, or the bottom if the reader is following. Reflow preserves the
message and pixel offset, not a semantic paragraph location.

The main conversation initially exposes the newest 40 chat items; older pages
are added near the top or through **Load earlier messages**. Loaded history keeps
its data and height estimates, not all of its SwiftUI views. The conversation has
two states: following the newest message and reading history. Scrolling up
releases the bottom; arriving back at the end or **Jump to latest** restores it.
Content growth alone cannot change this state. Page Up/Down and beginning/end
commands use the same policy. This implementation supports macOS 14 and later.
An outstanding wheel-state report takes precedence over an older SwiftUI binding
update, so it cannot be mistaken for a command to jump back to the end. Height
reports carry a hosting-root generation and are coalesced; retired/replaced roots
cannot correct the current scroll position. Follow-state and chrome-inset changes
do not rebuild unchanged message roots.

Completed message rows and Markdown blocks skip redundant body work. Both Apple
renderers use MarkdownBlocks for tables, rules and repairing unfinished Markdown.
The Mac does not animate paragraph interpolation as tokens arrive. Reply threads
use the same following rules as the main transcript.

Closed anchored menus report a constant zero geometry instead of writing their
global position into state on every scroll frame. Only an open menu tracks its
anchor. Native hosted rows convert their anchors to window coordinates before
presentation. This applies to Mac message context menus and dropdowns; iPhone message
context menus already use the system implementation.

## Appearance and controls

The Mac has a single composer capsule containing attachment, input and send/stop/
call controls. Chat text uses a local typography adjustment without changing
shared typography defaults or iPhone chrome. Agent bubbles reserve space beside
them for compact reaction, reply and more actions. Hover controls do not change
message height. Message dates remain available in tooltips.

On Mac, `conversationInset` overlays the header and composer on a full-height
native viewport. Their measured heights become document padding rather than
shrinking the scroll view. Messages can therefore pass underneath both controls,
through mirrored material/gradient fades. The last message still clears the
composer at the bottom, and the jump button sits above it. On iPhone this helper
delegates to the existing system safe-area inset with unchanged spacing.

`<<` opens Details, replacing an open reply thread in the inspector slot. `>>`
closes Details. Both retain the conversation view and its following state.
Opening the inspector does not interpolate paragraph wrapping. A compact Details
presentation dismisses when the window becomes wide enough for the inspector.
Inspector transitions use a short 20-point slide and fade with `Motion.morph`.
Floating chevrons, inspector actions and Jump to latest share one clipped circular
surface, including hover and press feedback. Directional arrows move subtly on
hover/press; pressing scales the surface to 94 percent. Reduce Motion disables
these translations and scaling. No rounded-rectangle button background is layered
outside the circle.

The smile action opens a compact horizontal reaction strip, using the same emoji
controls as the message's context menu. Short replies keep their action cluster
beside the bubble, inside its hover region. Menu presentation waits for a nonzero
anchor before the Mac modal host captures its contents. iPhone keeps its native
context menu; Linux GTK and TUI keep their existing reaction pickers.

## Platform scope and verification

The iPhone implementations were inspected: they already have separate paging,
streaming and gesture-aware following. Their control placement and typography
remain unchanged. Linux GTK (`ui.rs`, `rows.rs`) and TUI (`app.rs`, `view.rs`) use
their own native scrolling/rendering; SwiftUI text interpolation, hover layout
and macOS inspector transitions do not apply. No protocol or action is added.

Validation includes geometry tests for 10,000 items and very tall messages,
native scroll-container tests that verify offscreen hosting replacement, stable
reading position during content growth, bottom following, and no SwiftUI root
rebuild while scrolling inside an already mounted message. macOS and iOS
Simulator builds cover the shared source. Computer-use checks cover native
scrolling, right-click anchors and inspector controls.
Regression tests also cover stale follow bindings, retired height reports,
synchronous viewport coverage and floating-chrome document padding.

Time Profiler recordings are diagnostic, not a controlled frame-time benchmark;
Computer Use's accessibility collection and recorder startup must be excluded
from scrolling samples. No 60/120 Hz or identical-to-Grok performance claim is
made. Real trackpad momentum, live streaming and attachment-heavy histories still
need broader user testing. Virtualization can discard offscreen row-local UI
state and text selection when its row leaves the overscan region.
