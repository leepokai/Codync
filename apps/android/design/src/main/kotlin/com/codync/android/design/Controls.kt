package com.codync.android.design

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
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
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.isSpecified
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

// Codync's own controls, as CodyncUI `Controls.swift` draws them. Anything with a background
// fill gets no border line.

/** Shrinks to 98% in 50 ms while pressed (Grok's press). */
@Composable fun Modifier.pressScale(pressed: Boolean): Modifier {
    val scale by animateFloatAsState(if (pressed) Motion.PRESS_SCALE else 1f, Motion.press(), label = "Press")
    return graphicsLayer { scaleX = scale; scaleY = scale }
}

/** Plain icon button that lights up on press. Icon-only, labelled for TalkBack. */
@Composable fun IconButton(title: String, systemImage: String, modifier: Modifier = Modifier, enabled: Boolean = true,
    tint: Color = Color.Unspecified, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    val label = tr(title)
    Box(modifier.size(36.dp).pressScale(pressed).clip(RoundedRectangle(10.dp))
        .background(Palette.text.copy(alpha = if (pressed) 0.14f else 0f))
        .clickable(source, null, enabled = enabled, role = Role.Button, onClick = action)
        .semantics { contentDescription = label }
        .graphicsLayer { alpha = if (enabled) 1f else 0.35f }, contentAlignment = Alignment.Center) {
        Symbol(systemImage, size = 21.dp, tint = if (tint.isSpecified) tint else Palette.secondary)
    }
}

/** A filled capsule button: `.buttonStyle(.primary)` (accent) or `.secondary`. */
@Composable fun PrimaryButton(title: String, modifier: Modifier = Modifier, enabled: Boolean = true, action: () -> Unit) =
    PrimaryButton(action, modifier, enabled) { Text(tr(title), maxLines = 1, overflow = TextOverflow.Ellipsis) }

@Composable fun SecondaryButton(title: String, modifier: Modifier = Modifier, enabled: Boolean = true, action: () -> Unit) =
    SecondaryButton(action, modifier, enabled) { Text(tr(title), maxLines = 1, overflow = TextOverflow.Ellipsis) }

/** `.buttonStyle(.primary)` around any label (the label sets its own font and frame). */
@Composable fun PrimaryButton(action: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, label: @Composable () -> Unit) =
    StyledButton(Palette.accentFill, Palette.onAccent, modifier, enabled, action, label)

@Composable fun SecondaryButton(action: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, label: @Composable () -> Unit) =
    StyledButton(Palette.bubbleUser, Palette.text, modifier, enabled, action, label)

@Composable private fun StyledButton(fill: Color, ink: Color, modifier: Modifier, enabled: Boolean, action: () -> Unit, label: @Composable () -> Unit) {
    val (source, pressed) = rememberPress()
    Box(modifier.pressScale(pressed).graphicsLayer { alpha = if (enabled) (if (pressed) 0.85f else 1f) else 0.4f }
        .clip(Capsule()).background(fill)
        .clickable(source, null, enabled = enabled, role = Role.Button, onClick = action)
        .padding(horizontal = 18.dp, vertical = 11.dp), contentAlignment = Alignment.Center) {
        CompositionLocalProvider(LocalTextStyle provides Type.body.medium, LocalContentColor provides ink) { label() }
    }
}

/** The text-only button iOS draws in the tint color (`Button(_:) .plain` links). */
@Composable fun LinkButton(title: String, modifier: Modifier = Modifier, enabled: Boolean = true, color: Color = Palette.text,
    style: TextStyle = Type.body, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    Text(tr(title), modifier.clickable(source, null, enabled = enabled, role = Role.Button, onClick = action)
        .graphicsLayer { alpha = if (!enabled) 0.4f else if (pressed) 0.5f else 1f }, style = style, color = color)
}

/** On/off switch: label on the left, a flat capsule track on the right. */
@Composable fun CodyncSwitch(checked: Boolean, onChange: (Boolean) -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true,
    label: @Composable RowScope.() -> Unit) {
    val track by animateColorAsState(if (checked) Palette.accentFill else Palette.bubbleUser, Motion.hover(), label = "Track")
    val knob by animateDpAsState(if (checked) 14.dp else 0.dp, Motion.hover(), label = "Knob")
    val knobInk = if (checked) Palette.onAccent else Color.White
    Row(modifier.toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onChange)
        .graphicsLayer { alpha = if (enabled) 1f else 0.4f },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        label()
        Spacer(Modifier.weight(1f))
        Box(Modifier.size(34.dp, 20.dp).clip(Capsule()).background(track).padding(2.dp)) {
            Box(Modifier.offset(x = knob).size(16.dp).clip(Capsule()).background(knobInk))
        }
    }
}

/** Loading indicator: a thin arc that turns. */
@Composable fun Spinner(size: Dp = 14.dp, modifier: Modifier = Modifier, color: Color = Palette.secondary) {
    val turn by rememberInfiniteTransition(label = "Spinner").animateFloat(0f, 360f,
        infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Restart), label = "Turn")
    val width = with(LocalDensity.current) { maxOf(1.5.dp, size / 9).toPx() }
    Canvas(modifier.size(size).rotate(turn).semantics { contentDescription = "Loading" }) {
        drawArc(color, 0f, 252f, false, style = Stroke(width, cap = StrokeCap.Round))
    }
}

/** Picks one of a few values side by side: the replacement for `.segmented`. */
@Composable fun <T> SegmentedChoice(selection: T, options: List<Pair<T, String>>, select: (T) -> Unit, modifier: Modifier = Modifier) {
    BoxWithConstraints(modifier.fillMaxWidth().clip(Capsule()).background(Palette.bubbleUser).padding(3.dp)) {
        val index = options.indexOfFirst { it.first == selection }
        val segment = (maxWidth - 2.dp * (options.size - 1)) / options.size
        val thumb by animateDpAsState((segment + 2.dp) * index, Motion.morph(), label = "Thumb")
        if (index >= 0) Box(Modifier.offset(x = thumb).width(segment).height(36.dp).clip(Capsule()).background(Palette.background))
        Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            for ((id, label) in options) {
                val on = id == selection
                Box(Modifier.width(segment).height(36.dp).clip(Capsule())
                    .selectable(on, role = Role.Tab) { select(id) }, contentAlignment = Alignment.Center) {
                    Text(tr(label), style = Type.subheadline.medium, color = if (on) Palette.text else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** Tabs as text, the chosen one on a filled pill (a details sheet's pages): `TabChoice`. */
@Composable fun <T> TabChoice(selection: T, options: List<Pair<T, String>>, select: (T) -> Unit, modifier: Modifier = Modifier) {
    // Four long labels (German) on a narrow phone scroll rather than cut a label short.
    Row(modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        for ((id, label) in options) {
            val on = id == selection
            val fill by animateColorAsState(if (on) Palette.bubbleAgent else Color.Transparent, Motion.morph(), label = "Tab")
            Text(tr(label), Modifier.clip(Capsule()).background(fill).selectable(on, role = Role.Tab) { select(id) }
                .padding(horizontal = 12.dp, vertical = 6.dp), style = Type.subheadline, color = if (on) Palette.text else Palette.secondary, maxLines = 1)
        }
    }
}

/** One choice per row with a checkmark: the replacement for `.inline` pickers. */
@Composable fun <T> ChoiceList(selection: T, options: List<Triple<T, String, String?>>, select: (T) -> Unit, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(14.dp)) {
        for ((id, label, detail) in options) {
            Row(Modifier.fillMaxWidth().selectable(id == selection, role = Role.RadioButton) { select(id) },
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(label, style = Type.body, color = Palette.text)
                    if (!detail.isNullOrEmpty()) Text(detail, style = Type.subheadline, color = Palette.secondary)
                }
                Symbol("checkmark", size = 15.dp, tint = Palette.text, modifier = Modifier.graphicsLayer { alpha = if (id == selection) 1f else 0f })
            }
        }
    }
}

/** Shows the current choice in a pill with a chevron and opens the menu: `ChoicePicker`. */
@Composable fun <T> ChoicePicker(selection: T, options: List<Pair<T, String>>, select: (T) -> Unit, modifier: Modifier = Modifier,
    fill: Color = Color.Transparent, label: String? = null, enabled: Boolean = true) {
    val rows = options.map { (id, text) -> id to tr(text) }
    val current = rows.firstOrNull { it.first == selection }?.second ?: tr("Choose")
    DropdownMenu({ rows.map { (id, text) -> MenuItem(text, selected = id == selection) { select(id) } } }, modifier,
        enabled = enabled, label = label?.let { "${tr(it)}: $current" }) {
        Row(Modifier.then(if (fill == Color.Transparent) Modifier.outline() else Modifier).pill(fill),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(current, Modifier.weight(1f, fill = false), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
        }
    }
}

/** A scrolling page of cards: the replacement for `Form` / grouped `List`. */
@Composable fun CardForm(modifier: Modifier = Modifier, padding: PaddingValues = PaddingValues(20.dp),
    content: @Composable ColumnScope.() -> Unit) {
    Column(modifier.fillMaxSize().background(Palette.background).verticalScroll(rememberScrollState()).padding(padding),
        verticalArrangement = Arrangement.spacedBy(32.dp)) {
        CompositionLocalProvider(LocalTextStyle provides Type.body, LocalContentColor provides Palette.text) { content() }
    }
}

/** The rows of a `CardSection`, split by inset hairlines. */
class CardRows internal constructor() {
    internal val rows = mutableListOf<@Composable () -> Unit>()
    fun row(content: @Composable () -> Unit) { rows += content }
}

/**
 * A titled group of rows: the replacement for `Section`. A small bold heading over a filled,
 * rounded group whose rows are split by inset hairlines (ChatGPT's desktop settings).
 */
@Composable fun CardSection(title: String? = null, footer: String? = null, modifier: Modifier = Modifier,
    accessory: @Composable RowScope.() -> Unit = {}, rows: CardRows.() -> Unit) {
    val list = CardRows().apply(rows).rows
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (title != null) Row(Modifier.padding(start = 2.dp), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionHeading(title, Modifier.weight(1f))
            accessory()
        }
        if (list.isNotEmpty()) Column(Modifier.fillMaxWidth().cardGroup()) {
            for ((index, row) in list.withIndex()) {
                Box(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp).heightIn(min = 32.dp),
                    contentAlignment = Alignment.CenterStart) {
                    CompositionLocalProvider(LocalTextStyle provides Type.body, LocalContentColor provides Palette.text) { row() }
                }
                if (index < list.lastIndex) Hairline(Modifier.padding(horizontal = 16.dp))
            }
        }
        if (footer != null) Text(tr(footer), Modifier.padding(horizontal = 2.dp), style = Type.caption.copy(lineHeight = Type.caption.lineHeight * 1.12f),
            color = Palette.secondary)
    }
}

/** How `CardSection` draws its group: filled (Settings style), or outlined on the page like Grok Bot's routine cards (the bot details sheet). */
enum class CardStyle { Filled, Outlined }

val LocalCardStyle = staticCompositionLocalOf { CardStyle.Filled }

/** A `CardSection` heading: small, bold, a heading to TalkBack. */
@Composable fun SectionHeading(title: String, modifier: Modifier = Modifier) =
    Text(tr(title), modifier.semantics { heading() }, style = Type.system(15f).semibold, color = Palette.text)

/** A `CardSection`'s group, drawn in the [LocalCardStyle]. */
@Composable fun Modifier.cardGroup(): Modifier =
    if (LocalCardStyle.current == CardStyle.Outlined) cardOutline() else clip(RoundedRectangle(16.dp)).background(Palette.surface)

/** Grok Bot's list cards (routines): an outline on the page, no fill. */
@Composable fun Modifier.outlinedCard(vertical: Dp = 12.dp): Modifier =
    fillMaxWidth().cardOutline().padding(horizontal = 14.dp, vertical = vertical)

@Composable private fun Modifier.cardOutline(): Modifier = border(1.dp, Palette.border, RoundedRectangle(14.dp))

/** A 1-pixel separator line in the border color. */
@Composable fun Hairline(modifier: Modifier = Modifier) {
    val px = with(LocalDensity.current) { 1f.toDp() }
    Box(modifier.fillMaxWidth().height(px).background(Palette.border))
}

/**
 * Label on the left, value on the right: the replacement for `LabeledContent`. A long value (a
 * language's full name) takes at most 60% of the row and truncates, so the label never breaks mid-word.
 */
@Composable fun ValueRow(label: String, modifier: Modifier = Modifier, detail: String? = null, value: @Composable () -> Unit) {
    Layout({
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(tr(label), style = Type.body, color = Palette.text)
            if (detail != null) Text(tr(detail), style = Type.caption.copy(lineHeight = Type.caption.lineHeight * 1.12f), color = Palette.secondary)
        }
        CompositionLocalProvider(LocalContentColor provides Palette.secondary, LocalTextStyle provides Type.body.copy(color = Palette.secondary)) { value() }
    }, modifier.fillMaxWidth()) { (labels, values), constraints ->
        val width = constraints.maxWidth
        val shown = values.measure(Constraints(maxWidth = (width * 0.6f).toInt()))
        val text = labels.measure(Constraints(maxWidth = (width - shown.width - 12.dp.roundToPx()).coerceAtLeast(0)))
        val height = maxOf(text.height, shown.height)
        layout(width, height) {
            text.place(0, (height - text.height) / 2)
            shown.place(width - shown.width, (height - shown.height) / 2)
        }
    }
}

@Composable fun ValueRow(label: String, value: String, modifier: Modifier = Modifier, detail: String? = null) =
    ValueRow(label, modifier, detail) { Text(tr(value), style = Type.body, color = Palette.secondary) }

/** A search box: the replacement for `.searchable`. */
@Composable fun SearchField(text: String, onChange: (String) -> Unit, modifier: Modifier = Modifier, prompt: String = "Search") {
    val clear = tr("Clear search")
    Row(modifier.fillMaxWidth().clip(Capsule()).background(Palette.bubbleAgent).padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol("magnifyingglass", size = 19.dp, tint = Palette.secondary)
        PlainTextField(text, onChange, Modifier.weight(1f), prompt, singleLine = true, imeAction = ImeAction.Search)
        if (text.isNotEmpty()) Box(Modifier.clickable(role = Role.Button) { onChange("") }.semantics { contentDescription = clear }) {
            Symbol("xmark.circle.fill", size = 18.dp, tint = Palette.tertiary)
        }
    }
}

/** `TextField(...).textFieldStyle(.plain)`: no chrome, a placeholder in the secondary ink. */
@Composable fun PlainTextField(text: String, onChange: (String) -> Unit, modifier: Modifier = Modifier, prompt: String = "",
    singleLine: Boolean = false, style: TextStyle = Type.body, minLines: Int = 1, maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    imeAction: ImeAction = ImeAction.Default, enabled: Boolean = true, keyboard: KeyboardOptions = KeyboardOptions(imeAction = imeAction),
    actions: KeyboardActions = KeyboardActions.Default) {
    BasicTextField(text, onChange, modifier, enabled = enabled, singleLine = singleLine, minLines = minLines, maxLines = maxLines,
        textStyle = style.copy(color = Palette.text), cursorBrush = SolidColor(Palette.accent), keyboardOptions = keyboard, keyboardActions = actions,
        decorationBox = { field ->
            val centered = style.textAlign == TextAlign.Center
            // A centered field spans its width, so its placeholder and empty cursor sit in the middle too.
            Box(contentAlignment = Alignment.CenterStart, propagateMinConstraints = centered) {
                if (text.isEmpty()) Text(tr(prompt), style = style, color = Palette.tertiary, maxLines = if (singleLine) 1 else Int.MAX_VALUE)
                field()
            }
        })
}

/** A row that expands to show more: the replacement for `DisclosureGroup`. */
@Composable fun Disclosure(expanded: Boolean, toggle: () -> Unit, modifier: Modifier = Modifier, label: @Composable RowScope.() -> Unit,
    content: @Composable ColumnScope.() -> Unit) {
    val turn by animateFloatAsState(if (expanded) 90f else 0f, Motion.layout(), label = "Chevron")
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = toggle)
            .semantics { stateDescription = if (expanded) "Expanded" else "Collapsed" },
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, content = label)
            Symbol("chevron.right", Modifier.rotate(turn), size = 13.dp, tint = Palette.secondary)
        }
        AnimatedVisibility(expanded, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
            exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) { Column(content = content) }
    }
}

/** The value pill: a filled rounded box, no border. */
@Composable fun Modifier.pill(fill: Color = Palette.background) =
    clip(RoundedRectangle(10.dp)).background(fill).padding(horizontal = 12.dp, vertical = 7.dp)

/** An outlined value pill, like ChatGPT's settings dropdowns (outline or fill, never both). */
@Composable fun Modifier.outline() = border(1.dp, Palette.border, RoundedRectangle(10.dp))
