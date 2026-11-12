package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import com.codync.android.core.DecisionStatus
import com.codync.android.core.Entry
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/** Selection and expiry come from the host, including answers made on another device. */
@Composable internal fun DecisionCard(entry: Entry, answering: Boolean, respond: (String) -> Unit) {
    val d = entry.data
    val pending = entry.decisionStatus == DecisionStatus.Pending
    val choices = entry.choices
    val suggested = entry.recommended
    var chosen by remember(entry.id) { mutableStateOf<String?>(null) }
    var expanded by remember(entry.id) { mutableStateOf(false) }
    // Whether the clamped question hides lines: only then is there something to expand.
    var clamped by remember(entry.id) { mutableStateOf(false) }
    val question = d.text("question").orEmpty()
    Column(Modifier.semantics { testTagsAsResourceId = true }
        .padding(end = 40.dp).widthIn(max = 420.dp).clip(RoundedRectangle(22.dp))
        .background(Palette.bubbleAgent).animateContentSize(Motion.layoutSize).padding(16.dp).testTag("decision-card"),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(tr("Needs your choice"), style = Type.headline, color = Palette.text)
            if (pending) Box(Modifier.size(7.dp).clip(Capsule()).background(Palette.warning))
        }
        Text(question, style = Type.subheadline, color = Palette.text, maxLines = if (expanded) Int.MAX_VALUE else 4,
            onTextLayout = { if (!expanded) clamped = it.hasVisualOverflow })
        if (pending) Column(Modifier.clip(RoundedRectangle(14.dp)).background(Palette.background)) {
            choices.forEachIndexed { index, choice ->
                if (index > 0) Box(Modifier.fillMaxWidth().height(0.5.dp).background(Palette.border))
                Row(Modifier.fillMaxWidth().heightIn(min = 46.dp)
                    .clickable(enabled = !answering, role = Role.Button) { chosen = choice; respond(choice) }
                    .graphicsLayer { alpha = if (!answering || chosen == choice) 1f else 0.4f }
                    .padding(horizontal = 14.dp, vertical = 6.dp).testTag("decision-option-$index"),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(choice, Modifier.weight(1f), style = if (suggested == index) Type.body.semibold else Type.body, color = Palette.text)
                    if (suggested == index) Text(tr("Suggested"), style = Type.caption, color = Palette.tertiary)
                    if (answering && chosen == choice) Spinner(14.dp)
                }
            }
        } else Text(if (entry.decisionStatus == DecisionStatus.Answered) d.text("selected") ?: tr("Answered") else tr("No answer — the bot moved on"),
            Modifier.testTag("decision-outcome"), style = Type.caption.medium, color = Palette.secondary)
        if (expanded || clamped) IconButton(if (expanded) "Collapse" else "Expand", if (expanded) "chevron.up" else "chevron.down",
            Modifier.testTag("decision-expand")) { expanded = !expanded }
    }
}
