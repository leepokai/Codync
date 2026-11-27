package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.WorkTask
import com.codync.android.design.*
import com.kyant.shapes.Capsule

/**
 * What a thread's agent runs in the background (a workflow, a command, a monitor), in one capsule directly above the
 * thread's composer, as the bot's work is over its chat ([WorkCapsule]): what the one piece does, or the names of
 * several, and for how long. Its task waits on that work, so the thread stays working between turns.
 */
@Composable internal fun BackgroundCapsule(task: WorkTask, modifier: Modifier = Modifier) {
    val label = backgroundLabel(task)
    // The capsule keeps its last words while it folds away.
    var shown by remember { mutableStateOf(label) }
    if (label != null && label != shown) shown = label
    AnimatedVisibility(label != null, modifier, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        val words = shown ?: return@AnimatedVisibility
        Row(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp).fillMaxWidth().glass(Capsule())
            .padding(horizontal = 14.dp, vertical = 5.dp).heightIn(min = 36.dp).semantics(mergeDescendants = true) {}
            .testTag("background-capsule"),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Spinner(12.dp)
            // The lead and the time stay whole; the middle gives way.
            Row(Modifier.weight(1f)) {
                Text(tr("Background") + " · ", style = Type.subheadline, color = Palette.text, maxLines = 1)
                Text(words.text, Modifier.weight(1f, fill = false), style = Type.subheadline, color = Palette.secondary,
                    maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Elapsed(words.since)
        }
    }
}
