package com.codync.android

import android.content.Context
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Graph
import com.codync.android.core.WorkflowNode
import com.codync.android.design.CodyncSheet
import com.codync.android.design.ModalHeader
import com.codync.android.design.Palette
import com.codync.android.design.Symbol
import com.codync.android.design.Type
import com.codync.android.design.medium
import com.codync.android.design.plainButton
import com.codync.android.design.tr

internal object StepDisplay {
    private const val FILE = "workflow-step-display"

    fun lines(context: Context): Boolean =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getBoolean("lines", false)

    fun set(context: Context, lines: Boolean) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit().putBoolean("lines", lines).apply()
    }
}

@Composable internal fun ModeButton(lines: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.plainButton(action = onClick).padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Symbol(if (lines) "arrow.triangle.branch" else "list.bullet.rectangle", size = 16.dp, tint = Palette.text)
        Text(tr("Mode"), style = Type.footnote.medium, color = Palette.text)
    }
}

@Composable internal fun WorkflowDisplaySheet(presented: Boolean, lines: Boolean, onDismiss: () -> Unit, choose: (Boolean) -> Unit) {
    CodyncSheet(presented, onDismiss, fitted = true) {
        Column(Modifier.fillMaxWidth().padding(bottom = 20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            ModalHeader("Dependency display")
            Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                ModeChoice(false, lines, choose)
                ModeChoice(true, lines, choose)
            }
        }
    }
}

@Composable private fun ModeChoice(lines: Boolean, selected: Boolean, choose: (Boolean) -> Unit) {
    val title = if (lines) "Line mode" else "Label mode"
    val detail = if (lines) "Show the path with arrows." else "Show where each step's input comes from."
    val symbol = if (lines) "arrow.triangle.branch" else "list.bullet.rectangle"
    Row(
        Modifier.fillMaxWidth().plainButton { choose(lines) }.background(Palette.surface, RoundedCornerShape(16.dp)).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.background(Palette.background, RoundedCornerShape(10.dp)).padding(10.dp), contentAlignment = Alignment.Center) {
            Symbol(symbol, size = 16.dp, tint = Palette.text)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(tr(title), style = Type.body.medium, color = Palette.text)
            Text(tr(detail), style = Type.footnote, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        if (lines == selected) Symbol("checkmark.circle.fill", size = 18.dp, tint = Palette.text)
    }
}

@Composable internal fun StepSources(parents: List<Int>, names: List<String> = emptyList(), modifier: Modifier = Modifier) {
    if (parents.isEmpty() && names.isEmpty()) return
    FlowRow(modifier, horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        for (name in names) {
            Text(tr("From {name}").replace("{name}", tr(name)), style = Type.footnote, color = Palette.tertiary, maxLines = 1)
        }
        for (parent in parents) {
            Text(
                tr("From step {number}").replace("{number}", parent.toString()),
                style = Type.footnote,
                color = Palette.tertiary,
                maxLines = 1,
            )
        }
    }
}

/** Steps in rows, with an arrow from each parent down to the step it feeds. */
@Composable internal fun WorkflowStepFlow(graph: Graph, names: (String) -> String?) {
    val numbers = stepNumbers(graph)
    val ranks = stepRanks(graph)
    var origin by remember { mutableStateOf<LayoutCoordinates?>(null) }
    val frames = remember { mutableStateMapOf<String, Rect>() }
    val ink = Palette.secondary.copy(alpha = 0.55f)
    Box(Modifier.fillMaxWidth().onGloballyPositioned { origin = it }) {
        Canvas(Modifier.matchParentSize()) {
            for (node in graph.nodes) {
                val child = frames[node.id] ?: continue
                for (parent in graph.parentsOf(node.id)) {
                    if (numbers[parent] == null) continue
                    val from = frames[parent] ?: continue
                    val start = Offset(from.center.x, from.bottom)
                    val end = Offset(child.center.x, child.top)
                    val mid = (start.y + end.y) / 2
                    val path = Path().apply {
                        moveTo(start.x, start.y)
                        cubicTo(start.x, mid, end.x, mid, end.x, end.y)
                    }
                    drawPath(path, ink, style = Stroke(width = 1.5.dp.toPx()))
                    val arrow = Path().apply {
                        moveTo(end.x, end.y)
                        lineTo(end.x - 5.dp.toPx(), end.y - 7.dp.toPx())
                        lineTo(end.x + 5.dp.toPx(), end.y - 7.dp.toPx())
                        close()
                    }
                    drawPath(arrow, ink)
                }
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(28.dp)) {
            for (rank in ranks) {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.Top) {
                    for (id in rank) {
                        val node = graph.nodes.firstOrNull { it.id == id } ?: continue
                        StepCard(node, names, numbers[id], graph.parentsOf(id).mapNotNull { numbers[it] }, Modifier.weight(1f).onGloballyPositioned { coords ->
                            origin?.let { root ->
                                val topLeft = root.localPositionOf(coords, Offset.Zero)
                                frames[id] = Rect(
                                    topLeft.x,
                                    topLeft.y,
                                    topLeft.x + coords.size.width.toFloat(),
                                    topLeft.y + coords.size.height.toFloat(),
                                )
                            }
                        })
                    }
                }
            }
        }
    }
}

@Composable private fun StepCard(node: WorkflowNode, names: (String) -> String?, number: Int?, parents: List<Int>, modifier: Modifier) {
    Column(modifier.background(Palette.surface, RoundedCornerShape(14.dp)).padding(12.dp)) {
        StepLine(node, names, number, parents)
    }
}
