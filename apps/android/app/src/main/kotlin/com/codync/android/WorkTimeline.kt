package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.WorkItem
import com.codync.android.core.workSections
import com.codync.android.design.*

/**
 * What the bot does, on its details' Details page (iOS `WorkTimeline`, with what it does later and keeps running in
 * between), top to bottom, empty sections left out: Threads (while an item is live or waiting: the agent sessions that
 * run now, of the most allowed, a faint row per place not in use), Event box (what waits for a thread, numbered in the
 * order it will be taken), Later (its follow-ups), Routines, Dev servers and Recent (items ended in the last day). A row
 * opens its item's thread (`openThread`); one that needs the user goes to its card in the chat (`openCard`; null: its
 * thread too). `routineId`: open on that routine's editor.
 */
@Composable internal fun WorkTimeline(bot: Bot, state: AppState, store: AppStore, routineId: String?, openCard: ((WorkItem) -> Unit)?,
    openThread: (WorkItem) -> Unit) {
    val work = workSections(bot.work, bot.maxAgents)
    WorkRowsHost(bot, state, store, openCard, openThread) { rows ->
        /** [items]' rows under [title] and what its heading carries ([count]); [numbered]: each row's turn; then a row per place [free]. */
        @Composable fun Section(title: String, items: List<WorkItem>, count: String? = null, numbered: Boolean = false, free: Int = 0) {
            if (items.isEmpty() && free == 0) return
            CardSection(title, accessory = { count?.let { Text(it, style = Type.footnote.monospacedDigit, color = Palette.tertiary) } }) {
                rows(items, numbered)
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
    }
}
