package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.core.UsageProvider
import com.codync.android.core.UsageReport
import com.codync.android.core.UsageWindow
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import kotlin.math.roundToInt

/**
 * Usage limits, opened from the State tab (iOS `UsageSheet` + `UsageView`): one card per
 * provider, the tightest limit as a big bar, every limit listed below. Pull down to refresh.
 */
@Composable fun UsageScreen(state: AppState?, refresh: (force: Boolean) -> Unit, reload: suspend () -> Unit,
    setShown: (id: String, shown: Boolean) -> Unit, trailing: @Composable () -> Unit = {}) {
    val dismiss = LocalDismissModal.current
    val report = remember(state?.usage) { state?.let { runCatching { UsageReport.decode(it.usage) }.getOrNull() } ?: UsageReport() }
    val online = state?.actualConnection is LinkState.Ready
    // Opening Usage asks the computer for a current report (it answers from cache when it can).
    LaunchedEffect(online) { if (online) refresh(false) }
    Column(Modifier.fillMaxSize()) {
        ScreenHeader(leading = { IconButton("Close", "xmark", action = dismiss) },
            title = { Text("Usage", style = Type.body.semibold, color = Palette.text) }, trailing = { trailing() })
        if (state == null) {
            EmptyUsage("No computer yet", "Usage shows up once a computer is connected.")
            return@Column
        }
        PullToRefresh({ if (online) reload() }, Modifier.weight(1f)) {
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                ProviderCards(report.providers)
                if (report.providers.isEmpty()) EmptyUsage("No usage yet", "Your computer reads plan limits from the agents your bots run and the ones you turn on.")
                if (report.services.isNotEmpty()) UsageChooser(report, setShown)
            }
        }
    }
}

@Composable private fun EmptyUsage(title: String, message: String) =
    EmptyState("chart.bar", title, Modifier.fillMaxWidth().padding(top = 60.dp), message)

/** The cards, each folding in or out as its agent turns on or off. */
@Composable private fun ProviderCards(providers: List<UsageProvider>) {
    val atFirst = remember { providers.map { it.id }.toSet() }
    // Every card seen, so that one turned off can still fold away.
    val seen = remember { java.util.TreeMap<String, UsageProvider>() }
    providers.forEach { seen[it.id] = it }
    for ((id, provider) in seen) key(id) {
        val showing = remember { MutableTransitionState(id in atFirst) }
        showing.targetState = providers.any { it.id == id }
        AnimatedVisibility(showing, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
            exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) { ProviderCard(provider) }
    }
}

/** Which agents' limits this computer lists. */
@Composable private fun UsageChooser(report: UsageReport, setShown: (id: String, shown: Boolean) -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(20.dp)).background(Palette.surface).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(tr("Show usage for"), style = Type.subheadline.semibold, color = Palette.text)
        for (service in report.services) key(service.id) {
            CodyncSwitch(service.shown, { setShown(service.id, it) }, Modifier.fillMaxWidth()) {
                ProviderTile(service.provider)
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(service.name, style = Type.subheadline, color = Palette.text)
                    if (service.shown && report.providers.none { it.id == service.id }) {
                        Text(tr("No reading yet"), style = Type.caption, color = Palette.tertiary)
                    }
                }
            }
        }
        Text(tr("On by default for the agents your bots run."), style = Type.caption, color = Palette.tertiary)
    }
}

/** An agent's mascot or logo on its tinted tile. */
@Composable private fun ProviderTile(provider: UsageProvider) {
    Box(Modifier.size(34.dp).clip(RoundedRectangle(10.dp)).background(provider.tint().copy(alpha = 0.12f)), contentAlignment = Alignment.Center) {
        ProviderMascot(provider, 22.dp)
    }
}

@Composable private fun ProviderCard(provider: UsageProvider) {
    var collapsed by rememberSaveable(provider.id) { mutableStateOf(false) }
    val turn by animateFloatAsState(if (collapsed) 180f else 0f, Motion.layout(), label = "Collapse")
    val tint = provider.widgetTint()
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(20.dp)).background(Palette.surface).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ProviderTile(provider)
            Text(provider.name, Modifier.weight(1f), style = Type.subheadline.semibold, color = Palette.text)
            IconButton(if (collapsed) "Expand" else "Collapse", "chevron.up", Modifier.rotate(turn)) { collapsed = !collapsed }
        }
        provider.tightest?.let { top ->
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row {
                    Text(tr(top.title), Modifier.weight(1f), style = Type.subheadline, color = Palette.text)
                    Text(usedText(top.percent), style = Type.subheadline.monospacedDigit, color = Palette.text)
                }
                UsageTicks(top.percent ?: 0.0, tint, 15.dp)
                val reset = top.resetsShort()
                if (reset != null) Text(resetLine(reset), Modifier.fillMaxWidth(), style = Type.caption.monospacedDigit, color = Palette.tertiary, textAlign = TextAlign.End)
            }
        }
        AnimatedVisibility(!collapsed, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
            exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (window in provider.windows) {
                    DetailRow(tr(window.title), reading(window))
                    val reset = window.resetsShort()
                    if (reset != null) DetailRow(tr("Resets"), resetLine(reset))
                }
                DetailRow("Source", provider.sourceLabel)
                DetailRow("Updated", provider.updatedAt?.takeIf { it > 0 }?.let(::updatedAt) ?: "Not reported")
            }
        }
    }
}

@Composable private fun reading(window: UsageWindow) = window.text?.takeIf { it.isNotBlank() } ?: usedText(window.percent)

/** "62% used": the share of the cap already spent, said so (a bare percent reads as what's left). */
@Composable private fun usedText(value: Double?) =
    value?.takeIf { it.isFinite() }?.let { tr("{percent}% used").replace("{percent}", "${it.roundToInt()}") } ?: "—"

/** "Oct 17, 2026 at 10:20 PM", as iOS formats an abbreviated date with a short time. */
private fun updatedAt(milliseconds: Long): String {
    val at = Instant.ofEpochMilli(milliseconds).atZone(ZoneId.systemDefault())
    val date = at.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
    val time = at.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT))
    return if (java.util.Locale.getDefault().language == "en") "$date at $time" else "$date $time"
}

@Composable private fun DetailRow(label: String, value: String) {
    Row {
        Text(tr(label), Modifier.weight(1f), style = Type.footnote, color = Palette.secondary)
        Text(value, style = Type.footnote.monospacedDigit, color = Palette.text)
    }
}
