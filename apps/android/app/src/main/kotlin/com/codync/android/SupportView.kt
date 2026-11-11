package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

/** How many log lines the page starts with and adds per tap of Show older events. */
private const val LOG_PAGE = 100

/**
 * Settings → Support: how to get help and the app's own recent events (OpenBot Mobile's Support
 * page). Nothing in the log leaves the phone unless Save log file is used.
 */
@Composable internal fun SupportView(store: AppStore, close: () -> Unit) {
    val uri = LocalUriHandler.current
    var showLog by remember { mutableStateOf(false) }
    var confirmClear by remember { mutableStateOf(false) }
    val clearLabel = tr("Clear")
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        if (showLog) SupportLogPage { showLog = false }
        else {
            ModalHeader(tr("Support")) { IconButton("Close", "xmark", action = close) }
            CardForm(padding = PaddingValues(start = 20.dp, top = 8.dp, end = 20.dp, bottom = 24.dp)) {
                CardSection(footer = tr("Codync never sends the log. It records no messages or files and masks passwords and tokens.")) {
                    row { Text(tr("The log stays on this phone. Save it and attach it when you ask for help, so the problem can be found."),
                        style = Type.subheadline, color = Palette.secondary) }
                    row { LinkButton(tr("Ask on Discord")) { uri.open("https://discord.gg/KBAX3Qznxh", OutboundLink.Discord) } }
                    row { LinkButton(tr("Report a problem on GitHub")) { uri.open("https://github.com/leepokai/Codync/issues", OutboundLink.Issues) } }
                }
                CardSection {
                    row { RowLabel("Show log", "doc.text", chevron = true, modifier = Modifier.plainButton { showLog = true }) }
                    row { RowLabel("Save log file", "square.and.arrow.up", modifier = Modifier.plainButton { store.saveSupportLog() }) }
                    row { RowLabel("Clear log", "trash", tint = Palette.danger, modifier = Modifier.plainButton { confirmClear = true }) }
                }
            }
        }
    }
    CodyncDialog(tr("Clear the log?"), confirmClear, { confirmClear = false },
        tr("The saved log keeps what was there; new events start the log again.")) {
        listOf(DialogAction(clearLabel, destructive = true) { SupportLog.clear() })
    }
}

/** The log itself, newest first with more revealed by a tap, live while the top is in view. */
@Composable private fun SupportLogPage(back: () -> Unit) {
    val events by SupportLog.events.collectAsState()
    var shown by remember { mutableIntStateOf(LOG_PAGE) }
    val list = rememberLazyListState()
    // New events at the top only while the reader is there, as the support log does.
    val newest = events.lastOrNull()?.id
    LaunchedEffect(newest) { if (newest != null && list.firstVisibleItemIndex <= 1) list.animateScrollToItem(0) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ScreenHeader({ BackButton(back) }, { Text(tr("Support log"), style = Type.body.semibold, color = Palette.text) }) {
            Text("${events.size}", style = Type.caption, color = Palette.tertiary)
        }
        if (events.isEmpty()) {
            Text(tr("No events recorded yet."), Modifier.padding(20.dp), style = Type.subheadline, color = Palette.secondary)
            return@Column
        }
        val descending = events.asReversed()
        LazyColumn(state = list, modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(horizontal = 20.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(descending.take(shown), key = { it.id }) { event -> SupportLogRow(event) }
            if (shown < descending.size) item("older") {
                SecondaryButton(tr("Show older events"), Modifier.fillMaxWidth()) { shown += LOG_PAGE }
            }
        }
    }
}

/** One line: its time, its level colored, and the event. */
@Composable private fun SupportLogRow(event: SupportEvent) {
    val tint = when (event.level) {
        SupportLevel.Error -> Palette.danger
        SupportLevel.Warn -> Palette.warning
        SupportLevel.Info -> Palette.secondary
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(event.clock(), style = Type.caption2.monospaced, color = Palette.tertiary)
        Column {
            Text("${event.source.name.uppercase()} · ${event.level.name.uppercase()}", style = Type.caption2.bold, color = tint)
            Text(event.message, Modifier.semantics { heading() }, style = Type.caption, color = Palette.text)
        }
    }
}
