package com.codync.android.design

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitLongPressOrCancellation
import androidx.compose.foundation.gestures.drag
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.roundToInt

/** One row in a menu: an action, a choice when `selected` is set, or a submenu with `children`. */
class MenuItem(
    val title: String,
    val icon: String? = null,
    val selected: Boolean? = null,
    val destructive: Boolean = false,
    val divider: Boolean = false,
    val enabled: Boolean = true,
    val children: List<MenuItem>? = null,
    /** Shows [title] as it is, never translated: a name the user gave (a section's). */
    val verbatim: Boolean = false,
    val detail: String? = null,
    val action: () -> Unit = {},
) {
    /** [title] as the row shows it: through [translate], unless [verbatim]. */
    fun shownTitle(translate: (String) -> String): String = if (verbatim) title else translate(title)
}

/** A row of emoji above a message's menu (Slack's quick reactions). */
class ReactionPick(val emoji: List<String>, val chosen: List<String>, val toggle: (String) -> Unit)

/** A button that opens the system-style glass menu, growing out of the button. */
@Composable fun DropdownMenu(items: () -> List<MenuItem>, modifier: Modifier = Modifier, enabled: Boolean = true,
    label: String? = null, content: @Composable BoxScope.() -> Unit) {
    var anchor by remember { mutableStateOf(Rect.Zero) }
    var open by remember { mutableStateOf(false) }
    Box(modifier.onGloballyPositioned { anchor = it.boundsInRoot() }
        .clickable(enabled = enabled, role = Role.Button, interactionSource = null, indication = null) { open = true }
        .then(if (label != null) Modifier.semantics { contentDescription = label } else Modifier),
        contentAlignment = Alignment.Center, content = content)
    MenuOverlay(open, { open = false }, anchor, items, cover = true)
}

/** Long-press then move: the row leaves its menu and follows the finger, as iOS list rows reorder. */
class ContextDrag(val start: () -> Unit, val move: (Float) -> Unit, val end: () -> Unit, val cancel: (() -> Unit)? = null)

/**
 * Long-press opens the menu under a lifted copy of the content; `reactions` adds the emoji row.
 * With `reorder`, moving the finger after the long press closes the menu and drags the content instead.
 */
@Composable fun Modifier.contextActions(reactions: ReactionPick? = null, enabled: Boolean = true, onClick: (() -> Unit)? = null,
    preview: (@Composable () -> Unit)? = null, reorder: ContextDrag? = null, items: () -> List<MenuItem>): Modifier {
    var anchor by remember { mutableStateOf(Rect.Zero) }
    var open by remember { mutableStateOf(false) }
    val haptics = LocalHapticFeedback.current
    if (preview != null) ContextOverlay(open, { open = false }, anchor, reactions, items, preview)
    else MenuOverlay(open, { open = false }, anchor, items, cover = false, reactions = reactions)
    val tap by rememberUpdatedState(onClick)
    val hold = {
        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
        open = true
    }
    val longPress = LocalViewConfiguration.current.longPressTimeoutMillis
    // One gesture for tap, hold and drag: a click detector would swallow the move after the hold.
    // A message reads as one node with its actions, as a clickable would merge it; a row that reorders keeps its own.
    val gestures = Modifier.semantics(mergeDescendants = reorder == null) {
        if (enabled) { onClick { tap?.invoke(); true }; onLongClick("Actions") { hold(); true } }
    }.pointerInput(reorder, enabled) {
        if (!enabled) return@pointerInput
        awaitEachGesture {
            // A message's hold counts on its links and buttons too, as the iPhone's context menu: even a press a child took.
            val down = awaitFirstDown(requireUnconsumed = reorder != null)
            val held = awaitLongPressOrCancellation(down.id)
            if (held == null) {
                // Not an unconsumed up: a child (a link, a button) took the tap, or the press became a scroll.
                val up = currentEvent.changes.firstOrNull { it.id == down.id }
                if (up != null && up.changedToUp() && !up.isConsumed) { up.consume(); tap?.invoke() }
                return@awaitEachGesture
            }
            hold()
            if (reorder == null) {
                // The rest of the press is kept from the child, so a held link opens the menu, not the page.
                do { awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() } } while (currentEvent.changes.any { it.pressed })
                return@awaitEachGesture
            }
            var moved = 0f
            var started = false
            // finally: a cancelled gesture (the row left the list, the handler restarted) still ends the drag.
            var completed = false
            try {
                completed = drag(held.id) { change ->
                    val step = change.positionChange().y
                    moved += step
                    if (!started && abs(moved) > viewConfiguration.touchSlop) { started = true; open = false; reorder.start() }
                    if (started) { reorder.move(step); change.consume() }
                }
            } finally {
                if (started) { if (completed) reorder.end() else (reorder.cancel ?: reorder.end)() }
            }
        }
    }
    // Keys as a clickable takes them (a row that reorders has none): Enter taps, a held Enter or the Menu key opens the menu.
    val keys = if (reorder != null || !enabled) Modifier else Modifier.onKeyEvent { event ->
        if (event.key !in listOf(Key.Enter, Key.NumPadEnter, Key.DirectionCenter, Key.Menu)) return@onKeyEvent false
        val key = event.nativeKeyEvent
        if (event.type == KeyEventType.KeyUp) { if (event.key == Key.Menu || key.eventTime - key.downTime >= longPress) hold() else tap?.invoke() }
        true
    }.focusable()
    return onGloballyPositioned { anchor = it.boundsInRoot() }.then(gestures).then(keys)
}

/** The menu panel and its placement around `anchor`. `cover`: grow over the anchor (pull-down menus). */
@Composable private fun MenuOverlay(open: Boolean, close: () -> Unit, anchor: Rect, items: () -> List<MenuItem>, cover: Boolean,
    reactions: ReactionPick? = null) {
    val shown = remember { Animatable(0f) }
    var mounted by remember { mutableStateOf(false) }
    LaunchedEffect(open) {
        if (open) { mounted = true; shown.animateTo(1f, Motion.layout()) }
        else { shown.animateTo(0f, Motion.fade()); mounted = false }
    }
    if (!mounted) return
    BackHandler(open, close)
    Overlay {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            Box(Modifier.fillMaxSize().clickable(remember { MutableInteractionSource() }, null, onClick = close))
            val density = LocalDensity.current
            val width = with(density) { maxWidth.toPx() }
            val height = with(density) { maxHeight.toPx() }
            MenuPlacement(anchor, width, height, cover, shown.value) {
                MenuPanel(items(), close, reactions)
            }
        }
    }
}

/** Puts the panel by its anchor: aligned to the anchor's side of the screen, below it (or above). */
@Composable private fun MenuPlacement(anchor: Rect, width: Float, height: Float, cover: Boolean, shown: Float,
    leading: Boolean = false, content: @Composable () -> Unit) {
    val density = LocalDensity.current
    val margin = with(density) { 12.dp.toPx() }
    val gap = with(density) { 8.dp.toPx() }
    var origin by remember { mutableStateOf(TransformOrigin.Center) }
    Box(Modifier.layout { measurable, constraints ->
        val placeable = measurable.measure(constraints.copy(minWidth = 0, minHeight = 0))
        val third = width / 3
        val x = when {
            leading || anchor.center.x < third -> anchor.left
            anchor.center.x > width - third -> anchor.right - placeable.width
            else -> anchor.center.x - placeable.width / 2f
        }.coerceIn(margin, (width - margin - placeable.width).coerceAtLeast(margin))
        val below = if (cover) anchor.top else anchor.bottom + gap
        val y = if (below + placeable.height <= height - margin) below
            else (if (cover) anchor.bottom else anchor.top - gap) - placeable.height
        val top = y.coerceIn(margin, (height - margin - placeable.height).coerceAtLeast(margin))
        origin = TransformOrigin(((anchor.center.x - x) / placeable.width).coerceIn(0f, 1f),
            if (top >= anchor.center.y) 0f else 1f)
        layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(x.roundToInt(), top.roundToInt()) }
    }.graphicsLayer {
        alpha = shown.coerceIn(0f, 1f)
        val scale = 0.55f + 0.45f * shown
        scaleX = scale; scaleY = scale
        transformOrigin = origin
    }) { content() }
}

private val MenuShape = RoundedRectangle(26.dp)

/** The glass panel of rows. A submenu replaces the rows, with its parent as a back row. */
@Composable private fun MenuPanel(items: List<MenuItem>, close: () -> Unit, reactions: ReactionPick?) {
    var submenu by remember { mutableStateOf<MenuItem?>(null) }
    Column(Modifier.width(250.dp).glass(MenuShape, GlassStyle.Menu).clip(MenuShape).padding(vertical = 10.dp)) {
        if (reactions != null) ReactionRow(reactions, close)
        AnimatedContent(submenu, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) }, label = "Submenu") { open ->
            Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState())) {
                if (open != null) {
                    MenuRow(MenuItem(open.title, icon = "chevron.left"), bold = true) { submenu = null }
                    MenuDivider()
                }
                for ((index, item) in (open?.children ?: items).withIndex()) {
                    if (item.divider && index > 0) MenuDivider()
                    MenuRow(item) {
                        if (item.children != null) submenu = item else { close(); item.action() }
                    }
                }
            }
        }
    }
}

@Composable private fun MenuDivider() {
    Box(Modifier.padding(horizontal = 26.dp, vertical = 8.dp).fillMaxWidth().height(1.dp).background(Palette.text.copy(alpha = 0.1f)))
}

@Composable private fun MenuRow(item: MenuItem, bold: Boolean = false, tap: () -> Unit) {
    val ink = when {
        !item.enabled -> Palette.text.copy(alpha = 0.3f)
        item.destructive -> Color(0xFFFF3B30)
        else -> Palette.text
    }
    val (source, pressed) = rememberPress()
    Row(Modifier.fillMaxWidth().heightIn(min = 42.dp)
        .padding(horizontal = 10.dp).clip(RoundedRectangle(14.dp))
        .background(if (pressed) Palette.text.copy(alpha = 0.08f) else Color.Transparent)
        .clickable(source, null, enabled = item.enabled, role = Role.Button, onClick = tap)
        .semantics { if (item.selected != null) selected = item.selected }
        .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(13.dp)) {
        val leading = when {
            item.icon != null -> item.icon
            item.selected == true -> "checkmark"
            item.selected == false -> null
            else -> item.icon
        }
        Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
            if (leading != null) Symbol(leading, size = 21.dp, tint = ink)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(item.shownTitle(translator()), style = if (bold) Type.body.semibold else Type.body, color = ink)
            item.detail?.let { Text(tr(it), style = Type.caption, color = Palette.secondary) }
        }
        if (item.icon != null && item.selected == true) Symbol("checkmark", size = 16.dp, tint = ink)
        if (item.children != null) Symbol("chevron.right", size = 16.dp, tint = Palette.secondary)
    }
}

@Composable private fun ReactionRow(reactions: ReactionPick, close: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp).padding(bottom = 6.dp), horizontalArrangement = Arrangement.SpaceBetween) {
        for (emoji in reactions.emoji) {
            val chosen = emoji in reactions.chosen
            Box(Modifier.size(38.dp).clip(RoundedRectangle(19.dp))
                .background(if (chosen) Palette.text.copy(alpha = 0.12f) else Color.Transparent)
                .clickable(role = Role.Button) { close(); reactions.toggle(emoji) }
                .semantics { contentDescription = emoji; selected = chosen }, contentAlignment = Alignment.Center) {
                Text(emoji, style = Type.title3)
            }
        }
    }
}

/** The long-press menu: the app blurred and dimmed, a lifted copy of the row, the menu under it. */
@Composable private fun ContextOverlay(open: Boolean, close: () -> Unit, anchor: Rect, reactions: ReactionPick?,
    items: () -> List<MenuItem>, preview: @Composable () -> Unit) {
    val shown = remember { Animatable(0f) }
    var mounted by remember { mutableStateOf(false) }
    LaunchedEffect(open) {
        if (open) { mounted = true; shown.animateTo(1f, Motion.layout()) }
        else { shown.animateTo(0f, Motion.fade()); mounted = false }
    }
    if (!mounted) return
    BackHandler(open, close)
    Overlay {
        val density = LocalDensity.current
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val height = with(density) { maxHeight.toPx() }
            val width = with(density) { maxWidth.toPx() }
            Box(Modifier.fillMaxSize().graphicsLayer { alpha = shown.value }
                .frost(14.dp, Color.Black.copy(alpha = if (LocalDark.current) 0.3f else 0.14f))
                .clickable(remember { MutableInteractionSource() }, null, onClick = close))
            // The lifted row: a rounded card in place, swelling slightly.
            Box(Modifier.offset { IntOffset(anchor.left.roundToInt(), anchor.top.roundToInt()) }
                .size(with(density) { anchor.width.toDp() }, with(density) { anchor.height.toDp() })
                .graphicsLayer { val s = 1f + 0.02f * shown.value; scaleX = s; scaleY = s }
                .clip(RoundedRectangle(16.dp)).background(Palette.background)) { preview() }
            MenuPlacement(anchor, width, height, cover = false, shown = shown.value, leading = true) { MenuPanel(items(), close, reactions) }
        }
    }
}
