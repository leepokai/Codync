package com.codync.android

import android.content.Context
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.glance.GlanceId
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.provideContent
import com.codync.android.core.OverviewConfig
import com.codync.android.core.WidgetApproval
import com.codync.android.core.WidgetFeed

/**
 * "Codync": one widget that shows the status, what needs you, the team, the usage limits and the last movement, as many of them as its
 * size allows, in the order the person chose (`overviewPlan`). The five older widgets stay as they are.
 */
class OverviewWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Responsive(setOf(SMALL, TALL, MEDIUM, LARGE, EXTRA_LARGE))
    override val previewSizeMode = sizeMode

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val widget = GlanceAppWidgetManager(context).getAppWidgetId(id)
        provideContent {
            // Recomposes when the report, a setting or a pending answer changes (`WidgetStore.changes`), not on a timer.
            val revision by WidgetStore.changes.collectAsState()
            val feed = remember(revision) { WidgetStore.feed(context) }
            val config = remember(revision) { WidgetStore.overview(context, widget) }
            key(revision) { OverviewCard(feed, config, sample = false) }
        }
    }

    /** The launcher's picker preview (Android 15+): sample data, drawn at the sizes the widget declares. */
    override suspend fun providePreview(context: Context, widgetCategory: Int) {
        provideContent { OverviewCard(sampleOverviewFeed(), OverviewConfig(), sample = true) }
    }

    internal companion object {
        val SMALL = DpSize(110.dp, 110.dp)
        val TALL = DpSize(110.dp, 250.dp)
        val MEDIUM = DpSize(250.dp, 110.dp)
        val LARGE = DpSize(250.dp, 250.dp)
        val EXTRA_LARGE = DpSize(400.dp, 250.dp)
    }
}

class OverviewWidgetReceiver : CodyncWidgetReceiver() { override val glanceAppWidget = OverviewWidget() }

/** Sample data for the picker and the setup screen: a request that can be answered here, one that can't, and a few bots. */
internal fun sampleOverviewFeed(): WidgetFeed {
    val now = System.currentTimeMillis()
    val bots = WidgetPreview.bots.mapIndexed { index, bot -> bot.copy(lastAt = now - (index + 1) * 4 * 60_000L) } +
        com.codync.android.core.WidgetBot("preview-research", "Research", "needsInput", avatarColor = "purple", avatarShape = "tablet", lastAt = now - 60_000L)
    return sampleFeed().copy(bots = bots, approvals = listOf(
        WidgetApproval("preview-review", "preview-entry-1", "allow_once", "reject_once", "low", now),
        WidgetApproval("preview-research", "preview-entry-2", "allow_once", "reject_once", "high", now)))
}
