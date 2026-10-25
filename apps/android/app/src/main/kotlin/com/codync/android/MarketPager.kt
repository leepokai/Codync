package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.CatalogPaging
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlin.math.abs
import kotlinx.coroutines.launch

// Extensions' sections and paging (iOS `MarketplaceTabs` + `CatalogPager`): the section rail, the pager a
// list turns in, and the bar under it.

/** Narrower than this, a list is a pager with previous and next beside it, and the bar keeps only the size and the page. */
private val PhonePagerWidth = 560.dp

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
 * On a phone the list is a pager: swipe or the side buttons turn it, and `settle` hears where it came to rest.
 * When the page moves from outside (search, page size, a failed fetch), the pager follows `page` back.
 * A wide layout shows the one page.
 */
@Composable internal fun FlankedPager(page: Int, count: Int, pageSize: Int, hasMore: Boolean = false, loading: Boolean = false,
    settle: (Int) -> Unit, content: @Composable (Int) -> Unit) {
    val scope = rememberCoroutineScope()
    val still = reduceMotion()
    val pages = CatalogPaging.pageCount(count, pageSize, hasMore)
    val current = CatalogPaging.clamp(page, count, pageSize, hasMore)
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        if (maxWidth >= PhonePagerWidth) content(current)
        else key(pageSize) {
            val pager = rememberPagerState(current) { pages }
            val onSettle by rememberUpdatedState(settle)
            LaunchedEffect(pager) { snapshotFlow { pager.settledPage }.collect { onSettle(it) } }
            LaunchedEffect(current, loading) {
                val from = pager.settledPage
                if (loading || from == current) return@LaunchedEffect
                if (still || abs(from - current) > 1) pager.scrollToPage(current) else pager.animateScrollToPage(current)
            }
            fun turn(by: Int) {
                scope.launch { (pager.currentPage + by).let { if (still) pager.scrollToPage(it) else pager.animateScrollToPage(it) } }
            }
            Box(Modifier.fillMaxWidth()) {
                HorizontalPager(pager, Modifier.fillMaxWidth(), pageSpacing = 20.dp, userScrollEnabled = !loading,
                    verticalAlignment = Alignment.Top) { content(it) }
                if (pages > 1) {
                    PageEdgeButton("Previous page", "chevron.left", current > 0 && !loading, Modifier.align(Alignment.CenterStart).offset(x = (-40).dp)) { turn(-1) }
                    PageEdgeButton(if (loading) "Loading…" else "Next page", "chevron.right", CatalogPaging.canForward(count, current, hasMore, pageSize) && !loading,
                        Modifier.align(Alignment.CenterEnd).offset(x = 40.dp)) { turn(1) }
                }
            }
        }
    }
}

/** Previous or next, beside the list on a phone: filled in the text color so it reads as a control, faint when there is no page that way. */
@Composable private fun PageEdgeButton(title: String, symbol: String, enabled: Boolean, modifier: Modifier = Modifier, action: () -> Unit) {
    Box(modifier.size(36.dp).graphicsLayer { alpha = if (enabled) 1f else 0.25f }.clip(CircleShape).background(Palette.text)
        .plainButton(enabled = enabled, action = action), contentAlignment = Alignment.Center) {
        Symbol(symbol, size = 15.dp, tint = Palette.background, contentDescription = tr(title))
    }
}

/** Count, page size, previous, current / total, and next. */
@OptIn(ExperimentalLayoutApi::class)
@Composable internal fun CatalogPager(page: Int, count: Int, pageSize: Int, hasMore: Boolean = false, loading: Boolean = false,
    go: (Int) -> Unit, onSize: (Int) -> Unit) {
    val pages = CatalogPaging.pageCount(count, pageSize, hasMore)
    val current = CatalogPaging.clamp(page, count, pageSize, hasMore)
    val total = tr("{count} total").replace("{count}", "$count")
    val canPrev = current > 0 && !loading
    val canNext = CatalogPaging.canForward(count, current, hasMore, pageSize) && !loading
    @Composable fun SizeLabel() {
        DropdownMenu(items = {
            CatalogPaging.pageSizes.map { size -> MenuItem("$size / page", selected = size == pageSize) { onSize(size) } }
        }) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(tr("$pageSize / page"), style = Type.footnote, color = Palette.secondary)
                Symbol("chevron.down", size = 10.dp, tint = Palette.secondary)
            }
        }
    }
    @Composable fun SizeMenu() {
        Text(tr("Per page"), style = Type.footnote, color = Palette.secondary)
        DropdownMenu(items = {
            CatalogPaging.pageSizes.map { size -> MenuItem("$size / page", selected = size == pageSize) { onSize(size) } }
        }) {
            Row(Modifier.clip(RoundedRectangle(8.dp)).background(Palette.bubbleAgent).padding(horizontal = 10.dp).height(28.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(tr("$pageSize / page"), style = Type.footnote, color = Palette.text)
                Symbol("chevron.down", size = 10.dp, tint = Palette.secondary)
            }
        }
    }
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        if (maxWidth < PhonePagerWidth) {
            // Previous and next sit beside the list; the capsule keeps the page size and the page number.
            Row(Modifier.fillMaxWidth().clip(Capsule()).background(Palette.bubbleAgent).padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
                SizeLabel()
                Text("${current + 1} / $pages", style = Type.subheadline.semibold, color = Palette.text)
            }
        } else FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp),
            itemVerticalAlignment = Alignment.CenterVertically) {
            Text(total, style = Type.footnote, color = Palette.secondary)
            SizeMenu()
            IconButton("Previous page", "chevron.left", enabled = canPrev) { go(current - 1) }
            Text("${current + 1} / $pages", style = Type.footnote, color = Palette.text)
            IconButton(if (loading) "Loading…" else "Next page", "chevron.right", enabled = canNext) { go(current + 1) }
        }
    }
}

