package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.AllWork
import com.codync.android.core.Bot
import com.codync.android.core.WorkTask
import com.codync.android.core.allWork
import com.codync.android.core.fill
import com.codync.android.core.workSections
import com.codync.android.design.*

/** A bot in the view of all bots' work, with its computer's state. */
internal class WorkingBot(val state: AppState, val bot: Bot)

/**
 * The bots of the computers the roster shows ([shown], in its order) that have a live or waiting task, each computer's
 * in the order its roster shows them (pinned, then section by section as [sectionOrder] has that computer's), a
 * folded section's or computer's included.
 */
internal fun allWork(shown: List<AppState>, sectionOrder: (AppState) -> List<String>): AllWork<WorkingBot> =
    allWork(shown.flatMap { state -> shownBots(roster(state.bots), sectionOrder(state)).map { WorkingBot(state, it) } }) { it.bot.work }

/**
 * The Threads tab (iOS `ThreadsTab`): every bot that has live or waiting tasks, the bot as a heading, which opens its
 * details, over the rows its details have for them, with the same actions. A row that needs the user opens the bot's
 * chat at its card; any other row of a task a thread has opens the bot's details at its thread. With none, an empty
 * state. [grouped]: the bots are on several computers, so a heading also names the bot's computer.
 */
@Composable internal fun ThreadsView(bots: List<WorkingBot>, accounts: AccountStore, bottomInset: Dp) {
    val grouped = bots.map { it.state.computer.id }.distinct().size > 1
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding()) {
        ScreenHeader(title = { Text(tr("Threads"), style = Type.body.semibold, color = Palette.text) })
        if (bots.isEmpty()) EmptyState("checklist", "No threads right now", Modifier.fillMaxSize(), message = "Tasks your bots work on show up here.")
        else Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = bottomInset),
            verticalArrangement = Arrangement.spacedBy(20.dp)) {
            for (working in bots) {
                val store = accounts.store(working.state.computer.id) ?: continue
                BotWork(working, store, grouped, openDetails = { store.openDetails(working.bot.id) },
                    openCard = { store.openCard(working.bot.id, it) }, openThread = { store.openDetails(working.bot.id, it.id) })
            }
        }
    }
}

/** One bot: its heading, then one card of its threads' rows and its event box's numbered rows. */
@Composable private fun BotWork(working: WorkingBot, store: AppStore, grouped: Boolean, openDetails: () -> Unit, openCard: (WorkTask) -> Unit,
    openThread: (WorkTask) -> Unit) {
    val (state, bot) = working.state to working.bot
    val work = workSections(bot.work, bot.maxThreads)
    val label = fill(tr("{name}, view details"), "name" to bot.name)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.plainButton(action = openDetails).clearAndSetSemantics { contentDescription = label }, verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            BotAvatar(bot, state.bots, 22.dp, animated = false)
            Text(if (grouped) "${bot.name} · ${state.hostName}" else bot.name, Modifier.weight(1f, fill = false), style = Type.body.semibold, color = Palette.text,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            Symbol("chevron.right", size = 12.dp, tint = Palette.tertiary)
        }
        WorkRowsHost(bot, state, store, openCard, openThread) { rows ->
            CompositionLocalProvider(LocalCardStyle provides CardStyle.Outlined) { CardSection { rows(work.threads, false); rows(work.box, true) } }
        }
    }
}
