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
import androidx.compose.foundation.layout.heightIn
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
import com.codync.android.core.WorkItem
import com.codync.android.design.*
import com.kyant.shapes.Capsule

/**
 * The bot's work away from the chat, directly above the composer, while any item is live or waiting: how many work
 * and wait, or what the one working item does and for how long. The text opens the bot's details ([open]); Stop, with
 * one item a task has, asks once, then stops it ([stop]).
 */
@Composable internal fun WorkCapsule(bot: Bot, stopping: Boolean, open: () -> Unit, stop: (WorkItem) -> Unit, modifier: Modifier = Modifier) {
    val label = capsuleLabel(bot.work, translator())
    // The capsule keeps its last words while it folds away.
    var shown by remember { mutableStateOf(label) }
    if (label != null && label != shown) shown = label
    var confirming by remember { mutableStateOf(false) }
    // The item Stop asked about: the one it stops, whatever started since.
    var asked by remember { mutableStateOf<WorkItem?>(null) }
    AnimatedVisibility(label != null, modifier, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        val words = shown ?: return@AnimatedVisibility
        Row(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp).fillMaxWidth().glass(Capsule())
            .padding(start = 14.dp, end = 5.dp, top = 5.dp, bottom = 5.dp).heightIn(min = 36.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(Modifier.weight(1f).plainButton(action = open).semantics(mergeDescendants = true) {},
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                WorkMark(words.mark)
                // The lead and the time stay whole; the middle gives way.
                Row(Modifier.weight(1f, fill = false)) {
                    Text(tr("In the background") + " · ", style = Type.subheadline, color = Palette.text, maxLines = 1)
                    Text(words.text, Modifier.weight(1f, fill = false), style = Type.subheadline, color = Palette.secondary,
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                words.since?.let { Elapsed(it) }
            }
            if (words.stops != null) {
                val title = tr("Stop")
                Box(Modifier.size(36.dp).clip(Capsule()).background(Palette.background).plainButton(enabled = !stopping) { asked = words.stops; confirming = true }
                    .semantics { contentDescription = title }, contentAlignment = Alignment.Center) {
                    Symbol("stop.fill", size = 12.dp, tint = Palette.text)
                }
            }
        }
    }
    StopWorkDialog(confirming, { confirming = false }) { asked?.let(stop) }
}

/** Asked once before a working item stops. */
@Composable internal fun StopWorkDialog(isPresented: Boolean, dismiss: () -> Unit, stop: () -> Unit) {
    CodyncDialog("Stop the background work?", isPresented, dismiss, message = "What it already changed stays.") {
        listOf(DialogAction("Stop", destructive = true, action = stop))
    }
}
