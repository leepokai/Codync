package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotTask
import com.codync.android.core.TasksSummary
import com.codync.android.core.fill
import com.codync.android.core.tasksSummary
import com.codync.android.design.*
import com.kyant.shapes.Capsule

/** A bot's background tasks in a few words, in the app's language: the capsule's and the roster's `<what>`. */
internal fun tasksWhat(tasks: List<BotTask>, tr: (String) -> String): String = when (val summary = tasksSummary(tasks)) {
    is TasksSummary.One -> if (summary.what.phrase) tr(summary.what.text) else summary.what.text
    is TasksSummary.Several -> fill(tr("{count} tasks running"), "count" to "${summary.count}") +
        if (summary.needsInput) " · " + tr("Needs you").replaceFirstChar(Char::lowercase) else ""
}

/** A roster row's preview while the chat itself is idle and tasks go on (iOS `rosterPreview`). */
internal fun rosterTaskLine(tasks: List<BotTask>, tr: (String) -> String): String =
    fill(tr("In the background: {what}"), "what" to tasksWhat(tasks, tr))

/**
 * The bot's work away from the chat, directly above the composer (iOS `TaskCapsule`): what it is doing and for how
 * long. The text opens the details ([open]); Stop asks once, then stops the one task or, with several, all of them ([stop]).
 */
@Composable internal fun TaskCapsule(bot: Bot, stopping: Boolean, open: () -> Unit, stop: (String?) -> Unit, modifier: Modifier = Modifier) {
    // The capsule keeps its last tasks while it folds away.
    var shown by remember { mutableStateOf(bot.tasks) }
    if (bot.tasks.isNotEmpty() && bot.tasks != shown) shown = bot.tasks
    var confirming by remember { mutableStateOf(false) }
    AnimatedVisibility(bot.tasks.isNotEmpty(), modifier, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        if (shown.isEmpty()) return@AnimatedVisibility
        Row(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp).fillMaxWidth().glass(Capsule())
            .padding(start = 14.dp, end = 5.dp, top = 5.dp, bottom = 5.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(Modifier.weight(1f).plainButton(action = open).semantics(mergeDescendants = true) {},
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Spinner(14.dp)
                // The lead and the time stay whole; the middle gives way.
                Row(Modifier.weight(1f, fill = false)) {
                    Text(tr("In the background") + " · ", style = Type.subheadline, color = Palette.text, maxLines = 1)
                    Text(tasksWhat(shown, translator()), Modifier.weight(1f, fill = false), style = Type.subheadline, color = Palette.secondary,
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Elapsed(shown.minOf { it.startedAt })
            }
            val label = tr("Stop")
            Box(Modifier.size(36.dp).clip(Capsule()).background(Palette.background).plainButton(enabled = !stopping) { confirming = true }
                .semantics { contentDescription = label }, contentAlignment = Alignment.Center) {
                Symbol("stop.fill", size = 12.dp, tint = Palette.text)
            }
        }
    }
    StopTasksDialog(confirming, { confirming = false }) { stop(bot.tasks.singleOrNull()?.id) }
}

/** Asked once before background work stops. */
@Composable internal fun StopTasksDialog(isPresented: Boolean, dismiss: () -> Unit, stop: () -> Unit) {
    CodyncDialog("Stop the background work?", isPresented, dismiss, message = "What it already changed stays.") {
        listOf(DialogAction("Stop", destructive = true, action = stop))
    }
}
