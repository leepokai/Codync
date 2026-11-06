package com.codync.android

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.FlowCanvas
import com.codync.android.core.RoutineFlow
import com.codync.android.core.RoutineFlow.Node
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * Nodes and links on a pannable, pinchable plane. The plane is laid out in dp (a node is 220 × 96) and its layer is scaled by
 * the view, whose translation is in pixels. A drag on a node moves it (saved on release); a drag elsewhere pans; a pinch zooms.
 */
@Composable internal fun FlowCanvasView(canvas: FlowCanvas, bots: Map<String, Bot>, editable: Boolean, select: (FlowSelection) -> Unit,
    move: (String, Double, Double) -> Unit) {
    val nodes = remember(canvas) { RoutineFlow.layout(canvas) }
    val edges = remember(canvas) { RoutineFlow.edges(canvas) }
    val density = LocalDensity.current.density
    var view by remember { mutableStateOf(RoutineFlow.View()) }
    var live by remember { mutableStateOf<Triple<String, Double, Double>?>(null) }
    var fitted by remember { mutableStateOf(false) }
    var size by remember { mutableStateOf(IntSize.Zero) }
    fun fitView(): RoutineFlow.View =
        RoutineFlow.fit(nodes, size.width / density, size.height / density).let { it.copy(x = it.x * density, y = it.y * density) }
    LaunchedEffect(size, nodes.size) {
        if (!fitted && size.width > 0 && nodes.isNotEmpty()) { fitted = true; view = fitView() }
    }
    fun placed(node: Node): Node = live?.takeIf { it.first == node.key }?.let { node.copy(x = it.second, y = it.third) } ?: node
    val byKey = nodes.associateBy { it.key }
    val dashed = Palette.tertiary
    val solid = Palette.accent
    Box(Modifier.fillMaxWidth().height(420.dp).clip(RoundedRectangle(16.dp)).background(Palette.surface)
        .onSizeChanged { size = it }
        .semantics { contentDescription = "Routine flow canvas" }
        .pointerInput(Unit) {
            detectTransformGestures { centroid, pan, zoom, _ ->
                val zoomed = RoutineFlow.zoomAt(view, zoom, centroid.x, centroid.y)
                view = zoomed.copy(x = zoomed.x + pan.x, y = zoomed.y + pan.y)
            }
        }) {
        Box(Modifier.fillMaxSize().graphicsLayer {
            translationX = view.x; translationY = view.y; scaleX = view.scale; scaleY = view.scale
            transformOrigin = TransformOrigin(0f, 0f)
        }) {
            Canvas(Modifier.size(1.dp)) {
                for (edge in edges) {
                    val from = byKey[edge.from]?.let(::placed) ?: continue
                    val to = byKey[edge.to]?.let(::placed) ?: continue
                    val x1 = (from.x + RoutineFlow.NODE_WIDTH).toFloat() * density
                    val y1 = (from.y + RoutineFlow.NODE_HEIGHT / 2).toFloat() * density
                    val x2 = to.x.toFloat() * density
                    val y2 = (to.y + RoutineFlow.NODE_HEIGHT / 2).toFloat() * density
                    val bend = maxOf(40f * density, abs(x2 - x1) / 2)
                    val path = Path().apply { moveTo(x1, y1); cubicTo(x1 + bend, y1, x2 - bend, y2, x2, y2) }
                    val start = edge.linkId == null
                    drawPath(path, if (start) dashed else solid, style = Stroke(width = 2f * density,
                        pathEffect = if (start) PathEffect.dashPathEffect(floatArrayOf(10f * density, 10f * density)) else null))
                }
            }
            for (node in nodes) {
                val at = placed(node)
                FlowNodeCard(node, at.x, at.y, canvas, bots, editable, select,
                    drag = { dx, dy ->
                        val base = live?.takeIf { it.first == node.key } ?: Triple(node.key, node.x, node.y)
                        live = Triple(node.key, base.second + dx, base.third + dy)
                    },
                    drop = { live?.takeIf { it.first == node.key }?.let { move(node.key, it.second, it.third) }; live = null })
            }
            // Each link has a small handle at the middle of its edge, so it can be selected on the canvas too.
            for (edge in edges.filter { it.linkId != null }) {
                val from = byKey[edge.from]?.let(::placed) ?: continue
                val to = byKey[edge.to]?.let(::placed) ?: continue
                val mx = (from.x + RoutineFlow.NODE_WIDTH + to.x) / 2 - 14
                val my = (from.y + to.y) / 2 + RoutineFlow.NODE_HEIGHT / 2 - 14
                Box(Modifier.offset { IntOffset((mx * density).roundToInt(), (my * density).roundToInt()) }.size(28.dp)
                    .clip(RoundedRectangle(14.dp)).background(Palette.background)
                    .clickable { select(FlowSelection.OfLink(edge.linkId!!)) }
                    .semantics { contentDescription = "Link" }, contentAlignment = Alignment.Center) {
                    Symbol("arrow.right", size = 14.dp, tint = Palette.secondary)
                }
            }
        }
        Row(Modifier.align(Alignment.BottomEnd).padding(8.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            IconButton(tr("Zoom out"), "minus") { view = RoutineFlow.zoomAt(view, 1f / 1.25f, size.width / 2f, size.height / 2f) }
            IconButton(tr("Zoom in"), "plus") { view = RoutineFlow.zoomAt(view, 1.25f, size.width / 2f, size.height / 2f) }
            IconButton(tr("Fit to view"), "arrow.up.left.and.arrow.down.right") { view = fitView() }
        }
    }
}

/** One node: a routine, or a bot with its avatar, model and the status of its latest step. */
@Composable private fun FlowNodeCard(node: Node, x: Double, y: Double, canvas: FlowCanvas, bots: Map<String, Bot>, editable: Boolean,
    select: (FlowSelection) -> Unit, drag: (Double, Double) -> Unit, drop: () -> Unit) {
    val density = LocalDensity.current.density
    val card = if (node.isRoutine) canvas.routines.firstOrNull { it.routine.id == node.id } else null
    val bot = if (node.isRoutine) null else bots[node.id]
    val title = card?.routine?.name ?: bot?.name ?: tr("Unknown bot")
    val step = if (node.isRoutine) null else RoutineFlow.latestStep(canvas, node.id)
    Column(Modifier.offset { IntOffset((x * density).roundToInt(), (y * density).roundToInt()) }
        .size(RoutineFlow.NODE_WIDTH.dp, RoutineFlow.NODE_HEIGHT.dp).clip(RoundedRectangle(12.dp))
        .background(if (node.isRoutine) Palette.bubbleAgent else Palette.background)
        .pointerInput(node.key, editable) {
            if (editable) detectDragGestures(onDragEnd = drop, onDragCancel = drop) { change, amount ->
                change.consume(); drag(amount.x.toDouble() / density, amount.y.toDouble() / density)
            }
        }
        .clickable { select(FlowSelection.OfNode(node.key)) }
        .semantics(mergeDescendants = true) { contentDescription = "${if (node.isRoutine) "Routine" else "Bot"}: $title" }
        .padding(10.dp), verticalArrangement = Arrangement.SpaceBetween) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (bot != null) BotAvatar(bot, bots.values.toList(), 30.dp) else Symbol("clock.arrow.circlepath", size = 26.dp, tint = Palette.secondary)
            Column(Modifier.weight(1f)) {
                Text(title, style = Type.system(13f), color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(card?.routine?.triggerDescriptions?.joinToString(" · ") ?: bot?.model ?: bot?.backend.orEmpty(),
                    style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (card != null) {
            val text = if (RoutineFlow.isUnconnected(canvas, card.routine.id)) "Not connected" else if (card.steps.isEmpty()) "No runs yet" else "Latest run"
            Text(tr(text), style = Type.caption, color = Palette.secondary)
        } else StatusChip(step?.status)
    }
}
