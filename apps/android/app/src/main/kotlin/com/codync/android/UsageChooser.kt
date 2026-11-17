package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.codync.android.core.UsageReport
import com.codync.android.core.UsageService
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import com.kyant.shapes.UnevenRoundedRectangle
import kotlinx.coroutines.launch

private val CardCorner = 20.dp

/** Readings and rows trade places with a short fade (iOS: the `List`'s, under `Motion.layout`). */
internal fun LazyItemScope.trading(): Modifier = Modifier.animateItem(Motion.fade(), Motion.layoutOffset, Motion.fade())

/** The agents' drags. What shows is the order just dropped until the host answers, so the row doesn't jump back. */
@Composable internal fun rememberUsageReorder(report: UsageReport, list: LazyListState, setOrder: suspend (ids: List<String>) -> Unit): ListReorder {
    val scope = rememberCoroutineScope()
    val reorder = remember { ListReorder() }
    var sent by remember { mutableStateOf<List<String>?>(null) }
    reorder.names = report.servicesIn(sent).map { it.id }
    reorder.position = { id -> list.layoutInfo.visibleItemsInfo.firstOrNull { it.key == id }?.offset?.toFloat() ?: 0f }
    // The list keeps its first visible item in place by key: left to that, a held row there never gets further from its slot and swaps on and on.
    reorder.beforeSwap = { list.requestScrollToItem(list.firstVisibleItemIndex, list.firstVisibleItemScrollOffset) }
    reorder.save = { ids ->
        sent = ids
        scope.launch {
            setOrder(ids)
            // A later drop keeps its own order on screen until its own answer.
            if (sent === ids) sent = null
        }
    }
    return reorder
}

/** One card of every agent, then the line under it. */
internal fun LazyListScope.chooserRows(report: UsageReport, services: List<UsageService>, reorder: ListReorder, setShown: (id: String, shown: Boolean) -> Unit) {
    itemsIndexed(services, key = { _, service -> service.id }) { index, service ->
        ChooserSlot(service, report.hasNoReading(service), reorder, setShown, first = index == 0, last = index == services.lastIndex)
    }
    item(key = "note") {
        Text(tr("On by default for the agents your bots run."), trading().padding(start = 16.dp, top = 8.dp), style = Type.caption, color = Palette.secondary)
    }
}

/** The row's place in the card, which stays filled while the row is lifted out of it and follows the finger. */
@Composable private fun LazyItemScope.ChooserSlot(service: UsageService, hasNoReading: Boolean, reorder: ListReorder, setShown: (id: String, shown: Boolean) -> Unit, first: Boolean, last: Boolean) {
    val lifted = reorder.dragged == service.id
    val top = if (first) CardCorner else 0.dp
    val bottom = if (last) CardCorner else 0.dp
    DisposableEffect(service.id) { onDispose { reorder.released(service.id) } }
    Box(Modifier.onSizeChanged { reorder.heights[service.id] = it.height }.then(if (lifted) Modifier.zIndex(1f) else trading())
        .background(Palette.surface, UnevenRoundedRectangle(topStart = top, topEnd = top, bottomStart = bottom, bottomEnd = bottom))) {
        Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).graphicsLayer { translationY = reorder.translation(service.id) }
            .then(if (lifted) Modifier.clip(RoundedRectangle(14.dp)).background(Palette.bubbleUser) else Modifier),
            verticalAlignment = Alignment.CenterVertically) {
            DragHandle(service.name, reorder.handle(service.id)) { reorder.move(service.id, it) }
            ChooserSwitch(service, hasNoReading) { setShown(service.id, it) }
        }
    }
}

@Composable private fun RowScope.ChooserSwitch(service: UsageService, hasNoReading: Boolean, change: (Boolean) -> Unit) {
    CodyncSwitch(service.shown, change, Modifier.weight(1f).heightIn(min = 52.dp).padding(end = 16.dp)) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ProviderMascot(service.provider, 20.dp)
            Text(service.name, Modifier.weight(1f, fill = false), style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (hasNoReading) Text(tr("No reading yet"), style = Type.subheadline, color = Palette.secondary, maxLines = 1)
        }
    }
}

/** Where a row is dragged from: at once, with no hold first, since a touch here means nothing else. TalkBack moves it a place at a time. */
@Composable private fun DragHandle(name: String, drag: ContextDrag, move: (Int) -> Unit) {
    val translate = translator()
    Box(Modifier.size(44.dp).pointerInput(drag) {
        detectVerticalDragGestures(onDragStart = { drag.start() }, onDragEnd = drag.end, onDragCancel = drag.cancel ?: drag.end) { change, step ->
            change.consume()
            drag.move(step)
        }
    }.semantics {
        contentDescription = translate("Move {name}").replace("{name}", name)
        customActions = moveActions(translate, move)
    }, contentAlignment = Alignment.Center) {
        Symbol("line.3.horizontal", size = 18.dp, tint = Palette.secondary)
    }
}
