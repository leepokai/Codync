package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.core.UsageProvider
import com.codync.android.core.UsageReport
import com.codync.android.core.UsageWindow
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlin.math.roundToInt

/**
 * One computer's usage, opened from the State tab (iOS `UsageSheet` + `UsageView`): its name, what its agents
 * have used, and behind a button which agents show and in what order. Pull down to refresh.
 */
@Composable fun UsageScreen(state: AppState?, refresh: (force: Boolean) -> Unit, reload: suspend () -> Unit,
    setShown: (id: String, shown: Boolean) -> Unit, setOrder: suspend (ids: List<String>) -> Unit, trailing: @Composable () -> Unit = {}) {
    val dismiss = LocalDismissModal.current
    val report = remember(state?.usage) { state?.let { runCatching { UsageReport.decode(it.usage) }.getOrNull() } ?: UsageReport() }
    val online = state?.actualConnection is LinkState.Ready
    // Opening Usage asks the computer for a current report (it answers from cache when it can).
    LaunchedEffect(online) { if (online) refresh(false) }
    Column(Modifier.fillMaxSize()) {
        ScreenHeader(leading = { IconButton("Close", "xmark", action = dismiss) },
            title = { Text("Usage", style = Type.body.semibold, color = Palette.text) }, trailing = { trailing() })
        if (state == null) {
            EmptyState("chart.bar", "No computer yet", Modifier.fillMaxWidth().padding(top = 60.dp), "Usage shows up once a computer is connected.")
            return@Column
        }
        PullToRefresh({ if (online) reload() }, Modifier.weight(1f)) { UsageSection(state, report, setShown, setOrder) }
    }
}

@Composable private fun UsageSection(state: AppState, report: UsageReport, setShown: (id: String, shown: Boolean) -> Unit, setOrder: suspend (ids: List<String>) -> Unit) {
    var arranging by rememberSaveable { mutableStateOf(false) }
    val canArrange = report.services.isNotEmpty()
    val editing = arranging && canArrange
    val list = rememberLazyListState()
    val reorder = rememberUsageReorder(report, list, setOrder)
    val services = report.servicesIn(reorder.shown)
    LazyColumn(Modifier.fillMaxSize(), list, PaddingValues(16.dp)) {
        item(key = "heading") { UsageHeading(state, canArrange, editing) { arranging = !editing } }
        when {
            editing -> chooserRows(report, services, reorder, setShown)
            report.providers.isEmpty() -> item(key = "none") {
                Text(tr("No usage information yet."), trading().padding(start = 16.dp, top = 8.dp), style = Type.subheadline, color = Palette.secondary)
            }
            else -> item(key = "limits") { UsageLimits(report.providers, trading()) }
        }
    }
}

/** The computer's name, and the one button that swaps the readings for the list of agents. */
@Composable private fun UsageHeading(state: AppState, canArrange: Boolean, editing: Boolean, toggle: () -> Unit) {
    // The start lines up with the mascots below, and the 44 dp target's glyph with the switches' edge.
    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).padding(start = 16.dp, end = 4.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        ComputerBadge(state.hello?.text("device"), 20.dp, state.computer.color)
        Text(state.hostName, Modifier.weight(1f), style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (canArrange) IconButton(if (editing) "Done" else "Choose and arrange", if (editing) "checkmark" else "slider.horizontal.3", Modifier.size(44.dp), action = toggle)
    }
}

@Composable private fun UsageLimits(providers: List<UsageProvider>, modifier: Modifier) {
    Column(modifier.fillMaxWidth().clip(RoundedRectangle(20.dp)).background(Palette.surface).animateContentSize(Motion.layoutSize)) {
        for (provider in providers) key(provider.id) { ProviderLimits(provider) }
    }
}

@Composable private fun ProviderLimits(provider: UsageProvider) {
    val tint = provider.widgetTint()
    Column(Modifier.padding(horizontal = 16.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ProviderMascot(provider, 20.dp)
            Text(provider.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        for (window in provider.windows) key(window.id) { WindowLimit(window, tint) }
    }
}

/** A limit's name and when it resets, how much is used, and under them the bar; or the note an agent gives instead. */
@Composable private fun WindowLimit(window: UsageWindow, tint: Color) {
    val note = window.text?.takeIf { it.isNotBlank() }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (note != null) {
                Text(tr(window.title), style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(note, Modifier.weight(1f), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis, textAlign = TextAlign.End)
            } else {
                Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(tr(window.title), Modifier.weight(1f, fill = false), style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(window.resetsShort()?.let { resetLine(it) }.orEmpty(), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                PercentUsed(window.reportedPercent)
            }
        }
        if (note == null) window.reportedPercent?.let { UsageTicks(it, tint, 5.dp) }
    }
}

/** "33%", read out as "33% used"; a dash when the agent didn't say, never a zero. */
@Composable private fun PercentUsed(percent: Double?) {
    val spoken = usedText(percent)
    Text(percent?.let { "${it.roundToInt()}%" } ?: "—", Modifier.semantics { contentDescription = spoken }, style = Type.subheadline.medium.monospacedDigit, color = Palette.text)
}

/** "62% used": the share of the cap already spent, said so (a bare percent reads as what's left). */
@Composable private fun usedText(value: Double?) =
    value?.let { tr("{percent}% used").replace("{percent}", "${it.roundToInt()}") } ?: "—"
