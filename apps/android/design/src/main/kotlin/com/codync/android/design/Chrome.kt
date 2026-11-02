package com.codync.android.design

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import kotlinx.coroutines.CancellationException
import kotlin.math.roundToInt

// Codync's screen chrome, as CodyncUI `Chrome.swift` has it: headers, back buttons, the tab bar
// and the push navigation of a `NavigationStack`.

/** A modal's title row: title, optional trailing actions, and a close button. */
@Composable fun ModalHeader(title: String, trailing: @Composable RowScope.() -> Unit = {}) =
    ModalHeader({ Text(tr(title), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }, trailing)

@Composable fun ModalHeader(title: @Composable RowScope.() -> Unit, trailing: @Composable RowScope.() -> Unit = {}) {
    val dismiss = LocalDismissModal.current
    Row(Modifier.fillMaxWidth().height(56.dp).padding(start = 20.dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.weight(1f).semantics(mergeDescendants = true) { heading() }, verticalAlignment = Alignment.CenterVertically) { title() }
        trailing()
        IconButton("Close", "xmark", action = dismiss)
    }
}

/** A full screen's top bar: leading, centered title, trailing. */
@Composable fun ScreenHeader(leading: @Composable RowScope.() -> Unit = {}, title: @Composable () -> Unit = {},
    trailing: @Composable RowScope.() -> Unit = {}) {
    Box(Modifier.fillMaxWidth().height(52.dp).background(Palette.background).padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
        Box(Modifier.semantics(mergeDescendants = true) { heading() }) { title() }
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            leading()
            Spacer(Modifier.weight(1f))
            trailing()
        }
    }
}

/** A screen with nothing to show (iOS `EmptyState`): a large symbol, a title and, under it, an optional line. */
@Composable fun EmptyState(symbol: String, title: String, modifier: Modifier = Modifier, message: String? = null) {
    Column(modifier.padding(horizontal = 24.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically)) {
        Symbol(symbol, size = 48.dp, tint = Palette.tertiary)
        Text(tr(title), style = Type.title3.semibold, color = Palette.text, textAlign = TextAlign.Center)
        message?.let { Text(tr(it), style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center) }
    }
}

/** The back button for a pushed screen. */
@Composable fun BackButton(action: () -> Unit) = IconButton("Back", "chevron.left", action = action)

/** A page's title row: the close button on a modal's first page, a back button once pushed in it. */
@Composable fun PageHeader(title: String, pop: (() -> Unit)? = null, icon: String? = null,
    trailing: @Composable RowScope.() -> Unit = {}) {
    val heading: @Composable () -> Unit = {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (icon != null) Symbol(icon, size = 24.dp, tint = Palette.text)
            Text(tr(title), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    if (pop != null) ScreenHeader({ BackButton(pop) }, heading, trailing)
    else ModalHeader({ heading() }, trailing)
}

/** One tab of the system tab bar. */
class TabItem<T>(val value: T, val title: String, val systemImage: String, val iconSize: Dp = 28.dp)

/** The iOS 26 tab bar: a floating glass capsule whose selection pill slides between tabs. */
@Composable fun <T> GlassTabBar(selection: T, tabs: List<TabItem<T>>, select: (T) -> Unit, modifier: Modifier = Modifier) {
    val index = tabs.indexOfFirst { it.value == selection }.coerceAtLeast(0)
    val pill by animateDpAsState(TAB_WIDTH * index, Motion.layout(), label = "Tab selection")
    Box(modifier.glass(Capsule()).padding(4.dp)) {
        Box(Modifier.offset(x = pill).size(TAB_WIDTH, 54.dp).clip(Capsule()).background(Palette.text.copy(alpha = 0.07f)))
        Row {
            for (tab in tabs) {
                val on = tab.value == selection
                Column(Modifier.size(TAB_WIDTH, 54.dp).clip(Capsule()).selectable(on, role = Role.Tab) { select(tab.value) },
                    horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                    Symbol(tab.systemImage, size = tab.iconSize, height = 28.dp, tint = Palette.text)
                    Text(tr(tab.title), style = Type.system(10f).medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

private val TAB_WIDTH = 90.dp

/**
 * `NavigationStack`: the top screen slides in from the right over the one below, which drifts
 * left and dims; the system back gesture drags it away like the iOS edge swipe.
 */
@Composable fun <T : Any> NavStack(stack: List<T>, key: (T) -> Any, pop: () -> Unit, modifier: Modifier = Modifier,
    content: @Composable (T) -> Unit) {
    val holder = rememberSaveableStateHolder()
    var shown by remember { mutableStateOf(stack) }
    val away = remember { Animatable(0f) }
    LaunchedEffect(stack) {
        when {
            stack.size > shown.size -> { shown = stack; away.snapTo(1f); away.animateTo(0f, Motion.present()) }
            stack.size < shown.size -> {
                if (away.value < 1f) away.animateTo(1f, Motion.present())
                shown = stack; away.snapTo(0f)
            }
            else -> shown = stack
        }
    }
    PredictiveBackHandler(enabled = stack.size > 1 && stack == shown) { events ->
        try {
            events.collect { away.snapTo(it.progress * 0.9f) }
            away.animateTo(1f, Motion.layout())
            pop()
        } catch (cancelled: CancellationException) {
            away.animateTo(0f, Motion.layout())
            throw cancelled
        }
    }
    BoxWithConstraints(modifier.fillMaxSize()) {
        val width = with(LocalDensity.current) { maxWidth.toPx() }
        val below = shown.getOrNull(shown.size - 2)
        if (below != null && away.value > 0f) Box(Modifier.fillMaxSize()
            .offset { IntOffset((-0.3f * width * (1f - away.value)).roundToInt(), 0) }) {
            holder.SaveableStateProvider(key(below)) { content(below) }
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.12f * (1f - away.value))))
        }
        val top = shown.lastOrNull() ?: return@BoxWithConstraints
        Box(Modifier.fillMaxSize().offset { IntOffset((width * away.value).roundToInt(), 0) }
            .graphicsLayer { shadowElevation = if (away.value > 0f) 16f else 0f }) {
            holder.SaveableStateProvider(key(top)) { content(top) }
        }
    }
}

/** A tappable row that dims while pressed (`Button { } .buttonStyle(.plain)`). */
@Composable fun Modifier.plainButton(enabled: Boolean = true, role: Role = Role.Button, action: () -> Unit): Modifier {
    val (source, pressed) = rememberPress()
    return clickable(source, null, enabled = enabled, role = role, onClick = action).graphicsLayer { alpha = if (pressed) 0.6f else 1f }
}
