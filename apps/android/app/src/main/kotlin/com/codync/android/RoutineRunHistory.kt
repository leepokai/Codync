package com.codync.android

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import com.codync.android.core.RoutineRun
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** Presentation state only. The host supplies the run order and all execution metadata. */
internal data class RoutineHistorySelection(val expanded: Boolean = false, val runId: String? = null)

/** A routine's saved run metadata. Browsing it has no execution or network side effects. */
@Composable internal fun RoutineRunHistory(
    routineName: String,
    runs: List<RoutineRun>,
    selection: RoutineHistorySelection,
    change: (RoutineHistorySelection) -> Unit,
) {
    val formatter = remember { DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM).withZone(ZoneId.systemDefault()) }
    val rotation by animateFloatAsState(if (selection.expanded) 90f else 0f, Motion.hover(), label = "Run history")
    val selectedIndex = runs.indexOfFirst { it.id == selection.runId }.coerceAtLeast(0)
    val selectedRun = runs.getOrNull(selectedIndex)
    Column(Modifier.fillMaxWidth().padding(bottom = 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth().semantics {
            stateDescription = if (selection.expanded) "Expanded" else "Collapsed"
        }.plainButton { change(selection.copy(expanded = !selection.expanded)) }.heightIn(min = 44.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("chevron.right", size = 10.dp, tint = Palette.secondary, modifier = Modifier.rotate(rotation))
            Text("Run history", Modifier.weight(1f), style = Type.caption.medium, color = Palette.secondary)
            Text("${runs.size}", style = Type.caption, color = Palette.secondary)
        }
        AnimatedVisibility(selection.expanded,
            enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
            exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                if (runs.isEmpty()) Text("No runs yet.", style = Type.caption, color = Palette.secondary)
                else {
                    for (run in runs.take(10)) {
                        val chosen = run.id == selectedRun?.id
                        Row(Modifier.fillMaxWidth().clip(RoundedRectangle(10.dp))
                            .background(if (chosen) Palette.bubbleUser else Palette.surface)
                            .semantics { selected = chosen }
                            .plainButton { change(selection.copy(runId = run.id)) }
                            .heightIn(min = 44.dp).padding(horizontal = 10.dp, vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text(formatter.format(Instant.ofEpochMilli(run.createdAt)), Modifier.weight(1f), style = Type.caption, color = Palette.text)
                            Text(run.label + (run.failureLabel?.let { " · $it" } ?: ""), style = Type.caption, color = if (run.status in listOf("failed", "interrupted")) Palette.danger else Palette.secondary)
                        }
                    }
                    if (selectedRun != null) {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            SecondaryButton("Older", enabled = selectedIndex < runs.lastIndex) {
                                runs.getOrNull(selectedIndex + 1)?.let { change(selection.copy(runId = it.id)) }
                            }
                            Text("${selectedIndex + 1} of ${runs.size}", Modifier.weight(1f), style = Type.caption, color = Palette.secondary)
                            SecondaryButton("Newer", enabled = selectedIndex > 0) {
                                runs.getOrNull(selectedIndex - 1)?.let { change(selection.copy(runId = it.id)) }
                            }
                        }
                        AnimatedContent(selectedRun, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) }, label = "Run details") { run ->
                            SelectionContainer {
                                Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                                    Text("$routineName · ${run.label}", Modifier.semantics { heading() }, style = Type.caption.medium, color = Palette.text)
                                    Text("Created: ${formatter.format(Instant.ofEpochMilli(run.createdAt))}", style = Type.caption, color = Palette.secondary)
                                    run.finishedAt?.let { Text("Finished: ${formatter.format(Instant.ofEpochMilli(it))}", style = Type.caption, color = Palette.secondary) }
                                    run.failureLabel?.let { Text("Reason: $it", style = Type.caption, color = Palette.danger) }
                                    run.scheduledFor?.let { Text("Scheduled for: ${formatter.format(Instant.ofEpochMilli(it))}", style = Type.caption, color = Palette.secondary) }
                                    run.missed?.takeIf { it.count > 0 }?.let {
                                        Text("Missed occurrences: ${it.count}${it.until?.let { until -> " · last at ${formatter.format(Instant.ofEpochMilli(until))}" } ?: ""}",
                                            style = Type.caption, color = Palette.secondary)
                                    }
                                    run.notBefore?.let { Text("Not before: ${formatter.format(Instant.ofEpochMilli(it))}", style = Type.caption, color = Palette.secondary) }
                                    run.detail?.takeIf { it.isNotBlank() }?.let { Text(it, style = Type.caption, color = Palette.text) }                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
