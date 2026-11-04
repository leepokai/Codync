package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.OverviewConfig
import com.codync.android.core.WidgetDensity
import com.codync.android.core.WidgetFeed
import com.codync.android.core.WidgetSection
import com.codync.android.core.WidgetSizeClass
import com.codync.android.core.WidgetTheme
import com.codync.android.core.overviewPlan
import com.codync.android.design.*

private fun sectionName(section: WidgetSection) = when (section) {
    WidgetSection.Status -> "Status"; WidgetSection.Attention -> "Needs you"; WidgetSection.Usage -> "Usage limits"
    WidgetSection.Team -> "Team"; WidgetSection.Recent -> "Last activity"
}

private fun sizeName(size: WidgetSizeClass) = when (size) {
    WidgetSizeClass.Small -> "Small"; WidgetSizeClass.Medium -> "Medium"; WidgetSizeClass.Large -> "Large"; WidgetSizeClass.ExtraLarge -> "Extra large"
}

/**
 * What the overview widget shows: which sections, in what order, whose bots, and its look. Changing any of it redraws the preview line
 * below (the sections each size keeps), which comes from the same `overviewPlan` the widget uses.
 */
@Composable internal fun OverviewSettings(config: OverviewConfig, feed: WidgetFeed, computers: List<Pair<String, String>>, change: (OverviewConfig) -> Unit) {
    val all = WidgetSection.entries
    val names = all.associateWith { tr(sectionName(it)) }
    val allBotsLabel = tr("All bots")
    val shown = config.sections
    val order = shown + all.filter { it !in shown }
    CardSection(title = "Sections", footer = "The first sections with something to show are the ones a small widget keeps.") {
        for ((index, section) in order.withIndex()) row {
            val on = section in shown
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(names.getValue(section), Modifier.weight(1f), style = Type.body, color = if (on) Palette.text else Palette.secondary)
                if (on) {
                    IconButton(tr("Move up"), "chevron.up", enabled = index > 0) { change(config.copy(sections = move(shown, section, -1)).normalized()) }
                    IconButton(tr("Move down"), "chevron.down", enabled = index < shown.lastIndex) { change(config.copy(sections = move(shown, section, 1)).normalized()) }
                }
                CodyncSwitch(on, { enabled ->
                    change(config.copy(sections = if (enabled) shown + section else shown - section).normalized())
                }, Modifier.width(34.dp).semantics { contentDescription = names.getValue(section) }, enabled = !on || shown.size > 1) {}
            }
        }
    }
    CardSection(title = "Whose", footer = "A widget follows one computer and, if you choose, only some of its bots.") {
        row {
            ValueRow("Computer") {
                ChoicePicker(config.computerId.orEmpty(), listOf("" to tr("Current computer")) + computers,
                    { change(config.copy(computerId = it.ifEmpty { null })) }, label = tr("Computer"))
            }
        }
        row {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(tr("All bots"), Modifier.weight(1f), style = Type.body, color = Palette.text)
                CodyncSwitch(config.botIds.isEmpty(), { all -> change(config.copy(botIds = if (all) emptyList() else feed.bots.take(OverviewConfig.MAX_BOTS).map { it.id })) },
                    Modifier.width(34.dp).semantics { contentDescription = allBotsLabel }) {}
            }
        }
        if (config.botIds.isNotEmpty()) for (bot in feed.bots) row {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(bot.name, Modifier.weight(1f), style = Type.body, color = Palette.text, maxLines = 1)
                CodyncSwitch(bot.id in config.botIds, { on ->
                    val next = if (on) config.botIds + bot.id else config.botIds - bot.id
                    // Unpicking the last bot would mean "all": keep one.
                    if (next.isNotEmpty()) change(config.copy(botIds = next))
                }, Modifier.width(34.dp).semantics { contentDescription = bot.name }) {}
            }
        }
    }
    CardSection(title = "Look") {
        row { ValueRow("Theme") { ChoicePicker(config.theme, listOf(WidgetTheme.System to tr("System"), WidgetTheme.Light to tr("Light"), WidgetTheme.Dark to tr("Dark")),
            { change(config.copy(theme = it)) }, label = tr("Theme")) } }
        row { ValueRow("Density") { ChoicePicker(config.density, listOf(WidgetDensity.Comfortable to tr("Comfortable"), WidgetDensity.Compact to tr("Compact")),
            { change(config.copy(density = it)) }, label = tr("Density")) } }
    }
    CardSection(title = "Preview", footer = "Sections each size keeps with today's data.") {
        for (size in WidgetSizeClass.entries) row {
            val plan = overviewPlan(feed, config, size)
            Column(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
                Text(tr(sizeName(size)), style = Type.footnote, color = Palette.secondary)
                Text(plan.columns.joinToString("  |  ") { column -> column.joinToString(" · ") { names.getValue(it.section) } },
                    style = Type.footnote, color = Palette.text)
            }
        }
    }
}

private fun move(sections: List<WidgetSection>, section: WidgetSection, by: Int): List<WidgetSection> {
    val at = sections.indexOf(section)
    val to = (at + by).coerceIn(0, sections.lastIndex)
    return sections.toMutableList().also { it.removeAt(at); it.add(to, section) }
}
