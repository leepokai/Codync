package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.core.LinkState
import com.codync.android.core.WorkAction
import com.codync.android.core.WorkItem
import com.codync.android.core.WorkStatus
import com.codync.android.core.workActions
import com.codync.android.core.workSections
import com.codync.android.design.*

/**
 * What the bot does, on its details' Details page (iOS `WorkTimeline`, with what it does later and keeps running in between), top to bottom, empty
 * sections left out: Threads (while an item is live or waiting: the agent sessions that run now, of the most allowed,
 * a faint row per place not in use), Event box (what waits for a thread, numbered in the order it will be taken), Later (its follow-ups), Routines,
 * Dev servers and Recent (items ended in the last day). A row opens its task's transcript; one that needs the user
 * goes to its card in the chat (`openCard`). `routineId`: open on that routine's editor.
 */
@Composable internal fun WorkTimeline(bot: Bot, state: AppState, store: AppStore, routineId: String?, openCard: (Entry) -> Unit) {
    val work = workSections(bot.work, bot.maxAgents)
    // A change reaches the computer one at a time, and only while it's there.
    val enabled = state.actualConnection is LinkState.Ready && workKey(bot.id) !in state.busy
    val chat = state.entries.filter { it.botId == bot.id }
    /** The item whose task's transcript is open, with that task. */
    var reading by remember(bot.id) { mutableStateOf<Pair<String, String>?>(null) }
    /** The item Stop or Cut in asks about. */
    var stopping by remember(bot.id) { mutableStateOf<WorkItem?>(null) }
    var cuttingIn by remember(bot.id) { mutableStateOf<WorkItem?>(null) }
    fun run(action: WorkAction, item: WorkItem) = when (action) {
        WorkAction.Stop -> stopping = item
        WorkAction.CutIn -> cuttingIn = item
        WorkAction.First -> store.workFirst(bot.id, item.id)
        WorkAction.Drop -> store.dropWork(bot.id, item.id)
        WorkAction.Retry -> store.retryWork(bot.id, item.id)
    }
    /** [items]' rows under [title] and what its heading carries ([count]); [numbered]: each row's turn; then a row per place [free]. */
    @Composable fun Section(title: String, items: List<WorkItem>, count: String? = null, numbered: Boolean = false, free: Int = 0) {
        if (items.isEmpty() && free == 0) return
        CardSection(title, accessory = { count?.let { Text(it, style = Type.footnote.monospacedDigit, color = Palette.tertiary) } }) {
            for ((index, item) in items.withIndex()) row {
                val card = if (item.status == WorkStatus.NeedsYou) pendingCard(chat, item) else null
                val open: (() -> Unit)? = if (card != null) { { openCard(card) } } else item.task?.let { task -> { reading = item.id to task } }
                WorkRow(item, cardLine(card) ?: itemLine(item, translator()), workActions(item, bot.work, bot.maxAgents), enabled, open,
                    number = (index + 1).takeIf { numbered }) { run(it, item) }
            }
            repeat(free) { row { FreeRow() } }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(28.dp)) {
        // Only while something is live or waiting: an idle bot's details have no row of free places.
        if (work.threads.isNotEmpty() || work.box.isNotEmpty()) Section("Threads", work.threads, count = "${work.threads.size} / ${work.places}", free = work.free)
        Section("Event box", work.box, count = "${work.box.size}", numbered = true)
        FollowUpsSection(bot, state, store)
        RoutinesView(bot, state, store, routineId)
        if (bot.devServers.isNotEmpty()) DevServersSection(bot, store)
        Section("Recent", work.recent)
    }
    CodyncSheet(reading, { reading = null }) { (itemId, taskId) -> TaskDetailsView(bot, itemId, taskId, state, store) }
    StopWorkDialog(stopping != null, { stopping = null }) { stopping?.let { store.stopWork(bot.id, it.id) } }
    CutInDialog(cuttingIn, work.threads, { cuttingIn = null }) { item, insteadOf -> store.workCutIn(bot.id, item.id, insteadOf.id) }
}
