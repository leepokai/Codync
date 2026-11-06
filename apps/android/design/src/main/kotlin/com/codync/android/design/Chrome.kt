package com.codync.android.design

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlin.math.roundToInt

// Codync's screen chrome, as CodyncUI `Chrome.swift` has it: headers, back buttons, the tab bar
// and the push navigation of a `NavigationStack`.

/** A modal's title row: title, optional trailing actions, and a close button. */
@Composable fun ModalHeader(title: String, trailing: @Composable RowScope.() -> Unit = {}) =
    ModalHeader({ Text(title, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }, trailing)

@Composable fun ModalHeader(title: @Composable RowScope.() -> Unit, trailing: @Composable RowScope.() -> Unit = {}) {
    val dismiss = LocalDismissModal.current
    Row(Modifier.fillMaxWidth().height(56.dp).padding(start = 20.dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.weight(1f).semantics(mergeDescendants = true) { heading() }, verticalAlignment = Alignment.CenterVertically) { title() }
        trailing()
        IconButton("Close", "xmark", action = dismiss)
    }
}

/** A full screen's top bar: leading, non-overlapping title slot, trailing. */
@Composable fun ScreenHeader(
    leading: @Composable RowScope.() -> Unit = {},
    title: @Composable () -> Unit = {},
    centerTitle: Boolean = false,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    Row(
        Modifier.fillMaxWidth().height(52.dp).background(Palette.background).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp), content = leading)
        Box(
            Modifier.weight(1f).semantics(mergeDescendants = true) { heading() },
            contentAlignment = if (centerTitle) Alignment.Center else Alignment.CenterStart,
        ) { title() }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp), content = trailing)
    }
}

/** The back button for a pushed screen. */
@Composable fun BackButton(action: () -> Unit) = IconButton("Back", "chevron.left", action = action)

/** A page's title row: the close button on a modal's first page, a back button once pushed in it. */
@Composable fun PageHeader(title: String, pop: (() -> Unit)? = null, trailing: @Composable RowScope.() -> Unit = {}) {
    if (pop != null) ScreenHeader({ BackButton(pop) },
        { Text(title, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }, trailing = trailing)
    else ModalHeader(title, trailing)
}

/** Visual style of the primary bottom navigation bar on compact screens. */
enum class NavBarStyle(val id: String, val title: String, val detail: String) {
    FloatingCapsule("capsule", "Floating capsule", "Liquid Glass capsule above content"),
    CompactDock("dock", "Compact dock", "Ergonomic floating dock with pill selection"),
    DockedSurface("surface", "Docked bar", "Full-width bottom navigation bar"),
    ;
    companion object {
        fun fromId(id: String?): NavBarStyle = entries.firstOrNull { it.id == id } ?: FloatingCapsule
    }
}

/** Semantic tone of a navigation badge or status dot. */
enum class TabBadgeTone { Accent, Warning, Working, Online, Offline }

/**
 * Presentation summary for a destination badge: visual count capped at `99+`, while screen
 * readers announce the full `accessibilityLabel` via `stateDescription`.
 */
data class TabBadge(
    val count: Int = 0,
    val tone: TabBadgeTone = TabBadgeTone.Accent,
    val pulse: Boolean = false,
    val accessibilityLabel: String? = null,
) {
    val visualText: String? get() = when {
        count > 99 -> "99+"
        count > 0 -> "$count"
        else -> null
    }
}

/** One tab of the system tab bar. */
class TabItem<T>(
    val value: T,
    val title: String,
    val systemImage: String,
    val iconSize: Dp = 28.dp,
    val badge: TabBadge? = null,
)

/** Renders the active bottom navigation bar style for the same destination model and badges. */
@Composable fun <T> CodyncBottomBar(
    selection: T,
    tabs: List<TabItem<T>>,
    select: (T) -> Unit,
    style: NavBarStyle = NavBarStyle.FloatingCapsule,
    bottomInset: Dp = 0.dp,
    modifier: Modifier = Modifier,
) {
    val tap = tabSwitchHaptics(selection, select)
    when (style) {
        NavBarStyle.FloatingCapsule -> GlassTabBar(selection, tabs, tap, modifier.padding(bottom = maxOf(bottomInset - 4.dp, 12.dp)))
        NavBarStyle.CompactDock -> CompactDockBar(selection, tabs, tap, modifier.padding(bottom = maxOf(bottomInset - 4.dp, 12.dp)))
        NavBarStyle.DockedSurface -> DockedSurfaceBar(selection, tabs, tap, bottomInset, modifier)
    }
}

/** `select` that ticks once when it moves to another tab (reselecting the current one stays silent). */
@Composable private fun <T> tabSwitchHaptics(selection: T, select: (T) -> Unit): (T) -> Unit {
    val haptics = LocalCodyncHaptics.current
    return { value ->
        if (value != selection) haptics.perform(CodyncHaptic.TabSwitch)
        select(value)
    }
}

/** The Liquid Glass capsule tab bar whose selection pill slides between tabs. */
@Composable fun <T> GlassTabBar(selection: T, tabs: List<TabItem<T>>, select: (T) -> Unit, modifier: Modifier = Modifier) {
    val index = tabs.indexOfFirst { it.value == selection }.coerceAtLeast(0)
    BoxWithConstraints(modifier.padding(horizontal = 12.dp), contentAlignment = Alignment.BottomCenter) {
        val count = tabs.size.coerceAtLeast(1)
        val defaultWidth = if (count <= 2) TAB_WIDTH else 84.dp
        val tabWidth = minOf(defaultWidth, (maxWidth - 8.dp) / count)
        val pill by animateDpAsState(tabWidth * index, Motion.layout(), label = "Tab selection")
        Box(Modifier.glass(Capsule()).padding(4.dp)) {
            Box(Modifier.offset(x = pill).size(tabWidth, 54.dp).clip(Capsule()).background(Palette.text.copy(alpha = 0.07f)))
            Row {
                for (tab in tabs) {
                    val on = tab.value == selection
                    Column(Modifier.size(tabWidth, 54.dp).clip(Capsule())
                        .tabSemantics(on, tab.badge) { select(tab.value) },
                        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                        TabIconWithBadge(tab, on)
                        Text(tab.title, style = Type.system(10f).medium, color = if (on) Palette.text else Palette.secondary,
                            maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

/** Ergonomic floating dock with rounded corners and an active icon+label pill. */
@Composable fun <T> CompactDockBar(selection: T, tabs: List<TabItem<T>>, select: (T) -> Unit, modifier: Modifier = Modifier) {
    val shape = RoundedRectangle(26.dp)
    BoxWithConstraints(modifier.padding(horizontal = 14.dp), contentAlignment = Alignment.BottomCenter) {
        val count = tabs.size.coerceAtLeast(1)
        val tabWidth = minOf(86.dp, (maxWidth - 12.dp) / count)
        Row(Modifier.glass(shape).padding(horizontal = 6.dp, vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
            for (tab in tabs) {
                val on = tab.value == selection
                val fill by animateColorAsState(if (on) Palette.text.copy(alpha = 0.09f) else Color.Transparent, Motion.hover(), label = "Dock tab")
                Column(Modifier.size(tabWidth, 52.dp).clip(RoundedRectangle(20.dp)).background(fill)
                    .tabSemantics(on, tab.badge) { select(tab.value) },
                    horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                    TabIconWithBadge(tab, on)
                    Text(tab.title, style = Type.system(10f).let { if (on) it.semibold else it.medium },
                        color = if (on) Palette.text else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** Full-width Android docked surface bar with top hairline and pill indicator. */
@Composable fun <T> DockedSurfaceBar(selection: T, tabs: List<TabItem<T>>, select: (T) -> Unit, bottomInset: Dp, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().background(Palette.surface.copy(alpha = 0.96f)).padding(bottom = bottomInset)) {
        Hairline()
        Row(Modifier.fillMaxWidth().height(60.dp).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            for (tab in tabs) {
                val on = tab.value == selection
                val pillFill by animateColorAsState(if (on) Palette.bubbleUser else Color.Transparent, Motion.hover(), label = "Surface pill")
                Column(Modifier.weight(1f).height(54.dp).clip(RoundedRectangle(16.dp))
                    .tabSemantics(on, tab.badge) { select(tab.value) },
                    horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                    Box(Modifier.width(52.dp).height(28.dp).clip(Capsule()).background(pillFill), contentAlignment = Alignment.Center) {
                        TabIconWithBadge(tab, on)
                    }
                    Spacer(Modifier.height(2.dp))
                    Text(tab.title, style = Type.system(10f).let { if (on) it.semibold else it.medium },
                        color = if (on) Palette.text else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** Vertical navigation rail for foldable, tablet and wide landscape windows (`>= 600.dp`). */
@Composable fun <T> CodyncNavigationRail(
    selection: T,
    tabs: List<TabItem<T>>,
    select: (T) -> Unit,
    modifier: Modifier = Modifier,
) {
    val tap = tabSwitchHaptics(selection, select)
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val bottom = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    Row(modifier.fillMaxHeight().background(Palette.surface)) {
        Column(
            Modifier.width(78.dp).fillMaxHeight().padding(top = top + 12.dp, bottom = bottom + 12.dp, start = 6.dp, end = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            for (tab in tabs) {
                val on = tab.value == selection
                val pillFill by animateColorAsState(if (on) Palette.bubbleUser else Color.Transparent, Motion.hover(), label = "Rail pill")
                Column(
                    Modifier.fillMaxWidth().height(58.dp).clip(RoundedRectangle(16.dp))
                        .tabSemantics(on, tab.badge) { tap(tab.value) },
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Box(Modifier.width(48.dp).height(28.dp).clip(Capsule()).background(pillFill), contentAlignment = Alignment.Center) {
                        TabIconWithBadge(tab, on)
                    }
                    Spacer(Modifier.height(3.dp))
                    Text(tab.title, style = Type.system(10f).let { if (on) it.semibold else it.medium },
                        color = if (on) Palette.text else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        Box(Modifier.width(with(LocalDensity.current) { 1f.toDp() }).fillMaxHeight().background(Palette.border))
    }
}

private fun Modifier.tabSemantics(selected: Boolean, badge: TabBadge?, onClick: () -> Unit): Modifier =
    selectable(selected, role = Role.Tab, onClick = onClick).semantics {
        if (!badge?.accessibilityLabel.isNullOrEmpty()) stateDescription = badge.accessibilityLabel
    }

@Composable private fun <T> TabIconWithBadge(tab: TabItem<T>, selected: Boolean) {
    val badge = tab.badge
    val reduced = reduceMotion()
    val pulse by rememberInfiniteTransition(label = "Tab badge").animateFloat(
        initialValue = 1f,
        targetValue = if (badge?.pulse == true && !reduced) 0.72f else 1f,
        animationSpec = infiniteRepeatable(tween(750), RepeatMode.Reverse),
        label = "Badge pulse",
    )
    Box(Modifier.width(42.dp).height(28.dp), contentAlignment = Alignment.Center) {
        Symbol(tab.systemImage, size = tab.iconSize, height = 28.dp, tint = if (selected) Palette.text else Palette.secondary)
        if (badge != null) {
            val fill = when (badge.tone) {
                TabBadgeTone.Warning -> Palette.warning
                TabBadgeTone.Working -> Palette.secondary
                TabBadgeTone.Online -> Palette.added
                TabBadgeTone.Offline -> Palette.warning
                TabBadgeTone.Accent -> Palette.accentFill
            }
            val ink = when (badge.tone) {
                TabBadgeTone.Warning -> Color(0xFF141414)
                TabBadgeTone.Working -> Palette.background
                else -> Palette.onAccent
            }
            val text = badge.visualText
            if (text != null) {
                Box(
                    Modifier.align(Alignment.TopEnd).offset(x = 4.dp, y = (-1).dp)
                        .graphicsLayer { if (badge.pulse && !reduced) alpha = pulse }
                        .height(15.dp).widthIn(min = 15.dp).clip(Capsule()).background(fill)
                        .padding(horizontal = 4.dp).clearAndSetSemantics {},
                    contentAlignment = Alignment.Center,
                ) {
                    Text(text, style = Type.system(9f).bold, color = ink, maxLines = 1)
                }
            } else {
                Box(
                    Modifier.align(Alignment.TopEnd).offset(x = 1.dp, y = 1.dp)
                        .graphicsLayer { if (badge.pulse && !reduced) alpha = pulse }
                        .size(8.dp).clip(Capsule()).background(fill).clearAndSetSemantics {}
                )
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
