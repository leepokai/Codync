package com.codync.android

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.Action
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.appWidgetBackground
import androidx.glance.appwidget.cornerRadius
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxHeight
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.semantics.contentDescription
import androidx.glance.semantics.semantics
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider as Ink
import com.codync.android.core.ApprovalPlan
import com.codync.android.core.OverviewConfig
import com.codync.android.core.OverviewPlan
import com.codync.android.core.OverviewState
import com.codync.android.core.PlannedSection
import com.codync.android.core.WidgetActions
import com.codync.android.core.WidgetBot
import com.codync.android.core.WidgetDensity
import com.codync.android.core.WidgetFeed
import com.codync.android.core.WidgetPage
import com.codync.android.core.WidgetSection
import com.codync.android.core.WidgetSizeClass
import com.codync.android.core.overviewPlan
import com.codync.android.core.reportAgeMinutes
import com.codync.android.core.scopedFeed
import com.codync.android.core.teamSlots
import com.codync.android.core.usageLimits
import com.codync.android.core.widgetSizeClass
import kotlin.math.roundToInt

// The "Codync" overview widget: one card that shows the sections `overviewPlan` picks for its size, in the person's order. Nothing here
// decides what to show or whether an action is allowed: that is `core` (WidgetOverview.kt), so it is tested without a launcher.

private fun text(size: Float, color: Ink, weight: FontWeight = FontWeight.Normal) = TextStyle(color, size.sp, weight)

@Composable internal fun OverviewCard(feed: WidgetFeed, config: OverviewConfig, sample: Boolean) {
    val context = LocalContext.current
    val size = LocalSize.current
    val ink = widgetInk(config.theme)
    val sizeClass = widgetSizeClass(size.width.value, size.height.value)
    val plan = overviewPlan(feed, config, sizeClass)
    val scoped = scopedFeed(feed, config)
    val compact = config.density == WidgetDensity.Compact
    val open = if (sample) GlanceModifier else GlanceModifier.clickable(actionStartActivity(widgetIntent(context, feed)))
    Box(GlanceModifier.fillMaxSize().appWidgetBackground().background(ink.ground).cornerRadius(22.dp).then(open)
        .padding(if (compact) 10.dp else 14.dp)) {
        if (plan.state == OverviewState.NoComputer) {
            Column(GlanceModifier.fillMaxSize()) {
                Text("Codync", style = text(13f, ink.ink, FontWeight.Medium), maxLines = 1)
                Spacer(GlanceModifier.defaultWeight())
                Text(phrase("Connect a computer in Codync."), style = text(12f, ink.secondary), maxLines = 3)
            }
        } else Row(GlanceModifier.fillMaxSize()) {
            plan.columns.forEachIndexed { index, column ->
                if (index > 0) {
                    Spacer(GlanceModifier.width(10.dp))
                    Box(GlanceModifier.width(1.dp).fillMaxHeight().background(ink.divider)) {}
                    Spacer(GlanceModifier.width(10.dp))
                }
                Column(GlanceModifier.defaultWeight().fillMaxHeight()) {
                    column.forEachIndexed { row, planned ->
                        if (row > 0) Spacer(GlanceModifier.height(if (compact) 5.dp else 8.dp))
                        Section(context, ink, planned, plan, feed, scoped, sizeClass, sample)
                    }
                }
            }
        }
    }
}

@Composable private fun Section(context: Context, ink: WidgetInk, planned: PlannedSection, plan: OverviewPlan, feed: WidgetFeed,
    scoped: WidgetFeed, size: WidgetSizeClass, sample: Boolean) {
    val go = { action: Action -> if (sample) GlanceModifier else GlanceModifier.clickable(action) }
    when (planned.section) {
        WidgetSection.Status -> StatusSection(ink, plan.state, scoped, size, go(actionStartActivity(
            if (scoped.bots.any { it.status == "needsInput" }) widgetIntent(context, feed, page = WidgetPage.Approvals) else widgetIntent(context, feed))))
        WidgetSection.Attention -> AttentionSection(context, ink, scoped, feed, planned.rows, size, sample)
        WidgetSection.Usage -> UsageSection(context, ink, scoped, planned.rows, go(actionStartActivity(widgetIntent(context, feed, usage = true))))
        WidgetSection.Team -> TeamSection(context, ink, feed, scoped, go(actionStartActivity(widgetIntent(context, feed, page = WidgetPage.Team))), sample)
        WidgetSection.Recent -> RecentSection(context, ink, feed, scoped, sample)
    }
}

/** How many need you and how many work, and how far to trust them. */
@Composable private fun StatusSection(ink: WidgetInk, state: OverviewState, feed: WidgetFeed, size: WidgetSizeClass, modifier: GlanceModifier) {
    val needs = feed.bots.count { it.status == "needsInput" || it.status == "error" }
    val working = feed.bots.count { it.status == "working" }
    val trusted = state == OverviewState.Fresh || state == OverviewState.Aging
    val age = reportAgeMinutes(feed)
    val note = when (state) {
        OverviewState.Offline -> phrase("Offline") + (age?.let { " · " + phrase("updated") + " " + phrase(ageWords(it)) } ?: "")
        OverviewState.Stale -> phrase("Old data") + (age?.let { " · " + phrase(ageWords(it)) } ?: "")
        OverviewState.NoData -> phrase("No data yet. Open Codync.")
        OverviewState.Aging -> phrase("Updated") + " " + phrase(ageWords(age ?: 0))
        else -> if ((age ?: 0) >= 1) phrase("Updated") + " " + phrase(ageWords(age ?: 0)) else phrase("Updated now")
    }
    val description = if (trusted) "$needs ${phrase("need you")}, $working ${phrase("working")}. $note" else note
    Column(modifier.fillMaxWidth().semantics { contentDescription = description }) {
        Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text("Codync", GlanceModifier.defaultWeight(), style = text(13f, ink.ink, FontWeight.Medium), maxLines = 1)
            if (size != WidgetSizeClass.Small) Text(note, style = text(10f, if (trusted) ink.secondary else ink.attention), maxLines = 1)
        }
        Spacer(GlanceModifier.height(6.dp))
        Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom) {
            Count(ink, if (trusted) "$needs" else "–", phrase(if (needs == 1) "Needs you" else "Need you"), if (needs > 0) ink.attention else ink.ink, GlanceModifier.defaultWeight())
            Count(ink, if (trusted) "$working" else "–", phrase("Working"), ink.ink, GlanceModifier.defaultWeight())
        }
        if (size == WidgetSizeClass.Small) Text(note, style = text(9f, if (trusted) ink.secondary else ink.attention), maxLines = 2)
    }
}

@Composable private fun Count(ink: WidgetInk, value: String, label: String, tint: Ink, modifier: GlanceModifier) {
    Column(modifier) {
        Text(value, style = text(32f, tint, FontWeight.Medium), maxLines = 1)
        Text(label, style = text(11f, ink.secondary), maxLines = 1)
    }
}

private fun stateLabel(bot: WidgetBot) = when (bot.status) { "needsInput" -> "Needs you"; "error" -> "Failed"; "working" -> "Working"; else -> "Ready" }

/** The bots that need you, each opening its chat; with actions when the widget is big enough to hold them. */
@Composable private fun AttentionSection(context: Context, ink: WidgetInk, scoped: WidgetFeed, feed: WidgetFeed, rows: Int, size: WidgetSizeClass, sample: Boolean) {
    val bots = scoped.bots.filter { it.status == "needsInput" || it.status == "error" }.sortedByDescending { it.lastAt }.take(rows)
    val actions = size == WidgetSizeClass.Large || size == WidgetSizeClass.ExtraLarge
    Column(GlanceModifier.fillMaxWidth()) {
        for ((index, bot) in bots.withIndex()) {
            if (index > 0) Spacer(GlanceModifier.height(6.dp))
            val open = if (sample) GlanceModifier else GlanceModifier.clickable(actionStartActivity(widgetIntent(context, feed, bot.id)))
            val stateText = phrase(stateLabel(bot))
            Row(open.fillMaxWidth().semantics { contentDescription = "${bot.name}, $stateText" }, verticalAlignment = Alignment.CenterVertically) {
                Face(context, bot.avatarShape, bot.avatarColor, 24.dp)
                Spacer(GlanceModifier.width(7.dp))
                Text(bot.name, GlanceModifier.defaultWeight(), style = text(11f, ink.ink, FontWeight.Medium), maxLines = 1)
                if (!actions) Text(stateText, style = text(10f, if (bot.status == "error") ink.danger else ink.attention), maxLines = 1)
                else Buttons(context, ink, bot, feed, sample)
            }
        }
    }
}

/** Reply, and Allow or Review plus Deny when the report holds this bot's request. What `WidgetActions` refuses to allow is "Review": it opens the app. */
@Composable private fun Buttons(context: Context, ink: WidgetInk, bot: WidgetBot, feed: WidgetFeed, sample: Boolean) {
    val approval = feed.approvals.firstOrNull { it.botId == bot.id }
    val sending = approval != null && WidgetStore.isSending(context, approval.entryId)
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (approval != null && sending) Text(phrase("Sending…"), style = text(10f, ink.secondary), maxLines = 1)
        else if (approval != null) {
            val allowHere = WidgetActions.approval(approval, allow = true) is ApprovalPlan.Send
            val params = { allow: Boolean -> actionParametersOf(EntryKey to approval.entryId, AllowKey to allow) }
            if (allowHere) Pill(phrase("Allow"), ink.accent, ink.onAccent, if (sample) null else actionRunCallback<ApprovalActionCallback>(params(true)), "${phrase("Allow")} ${bot.name}")
            else Pill(phrase("Review"), ink.tile, ink.ink, if (sample) null else actionStartActivity(widgetIntent(context, feed, bot.id)), "${phrase("Review")} ${bot.name}")
            if (approval.denyOptionId != null) {
                Spacer(GlanceModifier.width(4.dp))
                Pill(phrase("Deny"), ink.tile, ink.ink, if (sample) null else actionRunCallback<ApprovalActionCallback>(params(false)), "${phrase("Deny")} ${bot.name}")
            }
        }
        Spacer(GlanceModifier.width(4.dp))
        val reply = android.content.Intent(context, WidgetQuickReplyActivity::class.java).putExtra(WidgetQuickReplyActivity.EXTRA_BOT, bot.id)
            .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        val messageLabel = "${phrase("Message")} ${bot.name}"
        Box(GlanceModifier.size(26.dp).background(ink.tile).cornerRadius(13.dp).then(if (sample) GlanceModifier else GlanceModifier.clickable(actionStartActivity(reply)))
            .semantics { contentDescription = messageLabel }, contentAlignment = Alignment.Center) {
            Image(ImageProvider(requireNotNull(com.codync.android.design.symbolRes("pencil"))), null, GlanceModifier.size(14.dp), colorFilter = ColorFilter.tint(ink.secondary))
        }
    }
}

@Composable private fun Pill(label: String, fill: Ink, ink: Ink, action: Action?, description: String) {
    Box(GlanceModifier.background(fill).cornerRadius(13.dp).padding(horizontal = 9.dp, vertical = 4.dp)
        .then(if (action == null) GlanceModifier else GlanceModifier.clickable(action)).semantics { contentDescription = description }, contentAlignment = Alignment.Center) {
        Text(label, style = text(10f, ink, FontWeight.Medium), maxLines = 1)
    }
}

/** The tightest limits, each as the thin ticks the usage widgets draw. */
@Composable private fun UsageSection(context: Context, ink: WidgetInk, feed: WidgetFeed, rows: Int, modifier: GlanceModifier) {
    val limits = usageLimits(feed.usage).take(rows)
    Column(modifier.fillMaxWidth()) {
        for ((index, limit) in limits.withIndex()) {
            val percent = limit.window.percent ?: 0.0
            if (index > 0) Spacer(GlanceModifier.height(5.dp))
            val limitName = "${limit.provider} ${phrase(limit.window.title)}"
            Row(GlanceModifier.fillMaxWidth().semantics { contentDescription = "$limitName ${percent.roundToInt()}%" }) {
                Text(limitName, GlanceModifier.defaultWeight(), style = text(10f, ink.secondary), maxLines = 1)
                Text("${percent.roundToInt()}%", style = text(10f, if (percent >= 90) ink.danger else ink.ink, FontWeight.Medium), maxLines = 1)
            }
            Ticks(context, percent, if (percent >= 70) 0xFFF0A030.toInt() else ink.ink.getColor(context).toArgb(), 9.dp)
        }
    }
}

/** Four bots as faces: the ones that need you first. */
@Composable private fun TeamSection(context: Context, ink: WidgetInk, feed: WidgetFeed, scoped: WidgetFeed, modifier: GlanceModifier, sample: Boolean) {
    val slots = teamSlots(scoped.bots, emptyList())
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        for ((index, bot) in slots.withIndex()) {
            if (index > 0) Spacer(GlanceModifier.width(8.dp))
            val open = if (sample || bot == null) GlanceModifier else GlanceModifier.clickable(actionStartActivity(widgetIntent(context, feed, bot.id)))
            Column(open.defaultWeight().semantics { contentDescription = bot?.name ?: "" }, horizontalAlignment = Alignment.CenterHorizontally) {
                if (bot == null) Face(context, "blob", "gray", 26.dp) else Face(context, bot.avatarShape, bot.avatarColor, 28.dp)
                Text(bot?.name ?: " ", style = text(9f, ink.ink), maxLines = 1)
            }
        }
    }
}

/** The last thing that moved: the bot that did something most recently. */
@Composable private fun RecentSection(context: Context, ink: WidgetInk, feed: WidgetFeed, scoped: WidgetFeed, sample: Boolean) {
    val bot = scoped.bots.filter { it.lastAt > 0 }.maxByOrNull { it.lastAt } ?: return
    val minutes = ((System.currentTimeMillis() - bot.lastAt) / 60_000L).coerceAtLeast(0)
    val open = if (sample) GlanceModifier else GlanceModifier.clickable(actionStartActivity(widgetIntent(context, feed, bot.id)))
    val lastLabel = phrase("Last activity")
    Row(open.fillMaxWidth().semantics { contentDescription = "$lastLabel: ${bot.name}" }, verticalAlignment = Alignment.CenterVertically) {
        Text(lastLabel, style = text(10f, ink.secondary), maxLines = 1)
        Spacer(GlanceModifier.width(6.dp))
        Text(bot.name, GlanceModifier.defaultWeight(), style = text(11f, ink.ink, FontWeight.Medium), maxLines = 1)
        Text(phrase(ageWords(minutes)), style = text(10f, ink.tertiary), maxLines = 1)
    }
}
