package com.codync.android

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.codync.android.core.ContextUsage
import com.codync.android.design.*
import java.text.NumberFormat

/** The main agent session's last reported context use, independent of voice and billing usage. */
@Composable internal fun ContextUsageIndicator(usage: ContextUsage?, diameter: Dp) {
    var showingDetails by remember { mutableStateOf(false) }
    val ink = when (usage?.level) {
        "warning" -> Palette.attention
        "danger" -> Palette.danger
        else -> Palette.secondary
    }
    val track = Palette.tertiary.copy(alpha = 0.35f)
    val label = if (usage == null) tr("Context usage unavailable") else
        tr("Context window: {percent}% used").replace("{percent}", usage.percent.toString())
    val (source, pressed) = rememberPress()
    AnchoredBubble(showingDetails, { showingDetails = false }, title = tr("Context window"), anchor = {
        Box(Modifier.size(44.dp).pressScale(pressed)
            .clickable(source, null, role = Role.Button) { showingDetails = true }
            .semantics(mergeDescendants = true) { contentDescription = label }, contentAlignment = Alignment.Center) {
            Canvas(Modifier.size(diameter)) {
                val stroke = 2.dp.toPx()
                drawCircle(track, radius = size.minDimension / 2 - stroke / 2,
                    style = Stroke(stroke, pathEffect = if (usage == null) PathEffect.dashPathEffect(floatArrayOf(2.dp.toPx(), 3.dp.toPx())) else null))
                if (usage != null) drawArc(ink, startAngle = -90f, sweepAngle = usage.percent * 3.6f, useCenter = false,
                    topLeft = Offset(stroke / 2, stroke / 2), size = Size(size.width - stroke, size.height - stroke),
                    style = Stroke(stroke, cap = StrokeCap.Round))
            }
            Text(usage?.let { "${it.percent}%" } ?: "–", Modifier.clearAndSetSemantics {}, style = Type.caption.copy(fontSize = 8.sp), color = ink)
        }
    }) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(tr("Context window"), style = Type.headline, color = Palette.text)
            if (usage != null) {
                Text("${usage.percent}%", style = Type.title2, color = ink)
                val numbers = NumberFormat.getIntegerInstance()
                Text(tr("{used} / {max} tokens")
                    .replace("{used}", numbers.format(usage.usedTokens))
                    .replace("{max}", numbers.format(usage.maxTokens)), style = Type.subheadline, color = Palette.text)
                Text(tr("Last reported by the agent."), style = Type.footnote, color = Palette.secondary)
            } else {
                Text(tr("The agent has not reported this session's context usage yet."), style = Type.footnote, color = Palette.secondary)
            }
        }
    }
}
