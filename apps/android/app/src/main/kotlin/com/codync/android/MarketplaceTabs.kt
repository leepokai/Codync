package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.launch

// Extensions' sections (iOS `MarketplaceTabs`): the section rail, and the end of a list the registry continues.

internal enum class ExtensionSection(val title: String, val icon: String, val source: String) {
    Agents("Agents", "terminal", "Agents come from the ACP registry and install on {name}."),
    Connectors("Connectors", "point.3.connected.trianglepath.dotted", "Connectors come from the official MCP Registry and install on {name}."),
    Skills("Skills", "book.pages", "Skills come from Anthropic and install on {name}."),
}

/** Agents, Connectors, Skills: the chosen row is filled, like a side tab. */
@Composable internal fun ExtensionRail(selection: ExtensionSection, select: (ExtensionSection) -> Unit) {
    Column(Modifier.width(168.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (section in ExtensionSection.entries) {
            val on = section == selection
            Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp))
                .background(if (on) Palette.bubbleAgent else Color.Transparent)
                .selectable(selected = on, role = Role.Tab) { select(section) }
                .padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Symbol(section.icon, size = 16.dp, tint = if (on) Palette.text else Palette.secondary)
                Text(tr(section.title), style = if (on) Type.subheadline.semibold else Type.subheadline,
                    color = if (on) Palette.text else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/**
 * The end of a list the registry continues (iOS `LoadMoreRow`), as the last item of a lazy list, which composes it
 * only near view. Composed, it asks for the page at `cursor`, and again each time the cursor moves on while it
 * stays. After a failure it shows Try again, and asks again once it scrolls back into view.
 */
@Composable internal fun LoadMoreRow(cursor: String, failed: Boolean, load: suspend () -> Unit) {
    val scope = rememberCoroutineScope()
    LaunchedEffect(cursor) { load() }
    if (failed) Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        IconButton("Try again", "arrow.clockwise") { scope.launch { load() } }
    } else SkeletonRows()
}
