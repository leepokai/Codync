package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AllWork
import com.codync.android.core.Bot
import com.codync.android.core.WorkTask
import com.codync.android.core.allWork
import com.codync.android.core.fill
import com.codync.android.core.workSections
import com.codync.android.design.*
import com.kyant.shapes.Capsule

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
 * Opens the view of all bots' work: in the roster's header only while some bot has a live or waiting task, with how
 * many tasks are worked on (no number at none).
 */
@Composable internal fun AllWorkButton(shown: Boolean, working: Int, open: () -> Unit) {
    AnimatedVisibility(shown, enter = fadeIn(Motion.fade()) + scaleIn(Motion.layout()), exit = fadeOut(Motion.fade()) + scaleOut(Motion.layout())) {
        val label = tr("Work")
        val count = fill(tr("{count} working"), "count" to "$working").takeIf { working > 0 }
        Box(Modifier.padding(end = 8.dp)) {
            Box(Modifier.size(44.dp).glass(Capsule()).plainButton(action = open)
                .semantics { contentDescription = label; count?.let { stateDescription = it } }, contentAlignment = Alignment.Center) {
                Symbol("tray.full", size = 24.dp, tint = Palette.text)
            }
            // Beside the glass, not in it: the glass clips its content to the circle.
            if (working > 0) Box(Modifier.align(Alignment.TopEnd).height(16.dp).widthIn(min = 16.dp).clip(Capsule()).background(Palette.accentFill)
                .padding(horizontal = 4.dp).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
                Text("$working", style = Type.caption2.bold, color = Palette.onAccent)
            }
        }
    }
}

/**
 * Every bot that has live or waiting tasks, in one sheet (the desktop's `AllWorkSheet`): the bot as a heading, which
 * opens its details ([openDetails]), over the rows its details have for them, with the same actions. A row that needs
 * the user opens the bot's chat at its card ([openCard]); any other row of a task a thread has opens the bot's details at
 * its thread. It closes when the last task ends. [grouped]: several computers show, so a heading also names the bot's computer.
 */
@Composable internal fun AllWorkSheet(open: Boolean, bots: List<WorkingBot>, grouped: Boolean, accounts: AccountStore, close: () -> Unit) {
    LaunchedEffect(open, bots.isEmpty()) { if (open && bots.isEmpty()) close() }
    CodyncSheet(open && bots.isNotEmpty(), close) {
        AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.AllWork))
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader("Work")
            Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
                for (working in bots) {
                    val store = accounts.store(working.state.computer.id) ?: continue
                    BotWork(working, store, grouped, openDetails = { close(); store.openDetails(working.bot.id) },
                        openCard = { close(); store.openCard(working.bot.id, it) }, openThread = { close(); store.openDetails(working.bot.id, it.id) })
                }
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
