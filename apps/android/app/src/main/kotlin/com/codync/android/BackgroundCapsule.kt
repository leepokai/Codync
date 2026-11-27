package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.WorkTask
import com.codync.android.design.*

/**
 * What a thread's agent runs in the background (a workflow, a command, a monitor), in one capsule directly above the
 * thread's composer, as the bot's work is over its chat ([WorkCapsule]): what the one piece does, or the names of
 * several, and for how long. Its task waits on that work, so the thread stays working between turns.
 */
@Composable internal fun BackgroundCapsule(task: WorkTask, modifier: Modifier = Modifier) {
    ComposerCapsule(backgroundLabel(task), modifier, row = Modifier.semantics(mergeDescendants = true) {}.testTag("background-capsule")) { words ->
        Spinner(12.dp)
        CapsuleLine(tr("Background"), words.text, Modifier.weight(1f))
        Elapsed(words.since)
    }
}
