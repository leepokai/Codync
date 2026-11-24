package com.codync.android

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.Capsule

// A thread the user starts from the Threads tab (the desktop's New thread), with a bot and on that bot's own settings:
// no folder, model or approval to pick, and no thread without a bot.

/** The bots a thread can start with now (desktop `threadBots`): [roster]'s own bots, no group chats, on computers online that start threads. */
internal fun threadBots(roster: List<WorkingBot>): List<WorkingBot> = roster.filter { of ->
    !of.bot.plain && of.bot.kind != "group" && of.state.actualConnection is LinkState.Ready && of.state.startsThreads
}

/** The Threads header's +: a menu of [bots] in roster order (each with its computer once they're on several); picking one [pick]s it. */
@Composable internal fun NewThreadButton(bots: List<WorkingBot>, pick: (WorkingBot) -> Unit) {
    val several = bots.distinctBy { it.state.computer.id }.size > 1
    DropdownMenu({ bots.map { of -> MenuItem(of.bot.name, verbatim = true, detail = of.state.hostName.takeIf { several }) { pick(of) } } },
        Modifier.size(44.dp).glass(Capsule()), label = tr("New thread")) {
        Symbol("plus", size = 30.dp, tint = Palette.text)
    }
}

/** The sheet a thread with [of]'s bot starts from: its first message, then Send, which opens the thread once it starts. */
@Composable internal fun NewThreadSheet(of: WorkingBot?, accounts: AccountStore, dismiss: () -> Unit) {
    val last = lastOf(of)
    CodyncSheet(of != null, dismiss, fitted = true) {
        val store = last?.let { accounts.store(it.state.computer.id) }
        if (last != null && store != null) NewThreadForm(last, store, dismiss)
    }
}

@Composable private fun NewThreadForm(of: WorkingBot, store: AppStore, dismiss: () -> Unit) {
    var text by remember(of.bot.id) { mutableStateOf("") }
    val state by store.state.collectAsState()
    val sending = startThreadKey(of.bot.id) in state.busy
    val title = tr("New thread with {name}").replace("{name}", of.bot.name)
    Column {
        ModalHeader({ Text(title, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }) {
            AnimatedContent(sending, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) }, label = "Send") { busy ->
                if (busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner(18.dp) }
                // A failure shows as every action's does and keeps the message here.
                else IconButton("Send", "arrow.up", enabled = text.isNotBlank()) { store.startThread(of.bot.id, text, dismiss) }
            }
        }
        CardSection(modifier = Modifier.padding(start = 20.dp, end = 20.dp, bottom = 20.dp)) {
            row { PlainTextField(text, { text = it }, Modifier.fillMaxWidth(), "Message", minLines = 3, maxLines = 8) }
        }
    }
}
