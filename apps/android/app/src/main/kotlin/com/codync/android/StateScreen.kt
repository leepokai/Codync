package com.codync.android

import android.Manifest
import android.app.NotificationManager
import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.Dp
import kotlinx.serialization.json.JsonObject
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.codync.android.core.UsageReport
import com.codync.android.core.WidgetKind
import com.codync.android.core.teamSlots
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** The surfaces outside the app, as the iOS State tab has Widget, Live Activity and Dynamic Island. */
private enum class Surface { Widget, LiveUpdate, StatusBar }

/**
 * Where your bots show up outside the app, each designed and switched on here: Home Screen
 * widgets, Live updates (Android's Live Activity) and the status bar chip (its Dynamic Island).
 * Usage limits open from the header.
 */
@Composable internal fun StateScreen(app: AccountContext, report: JsonObject?, accounts: AccountStore?, usage: () -> Unit, bottomInset: Dp, active: Boolean = true) {
    var surface by rememberSaveable { mutableStateOf(Surface.Widget) }
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding()) {
        ScreenHeader(title = { Text("State", style = Type.body.semibold, color = Palette.text) },
            trailing = { IconButton("Usage limits", "chart.bar", action = usage) })
        SegmentedChoice(surface, listOf(Surface.Widget to "Widget", Surface.LiveUpdate to "Live update", Surface.StatusBar to "Status bar"),
            { surface = it }, Modifier.padding(horizontal = 16.dp).padding(bottom = 8.dp))
        AnimatedContent(surface, Modifier.weight(1f), transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) },
            label = "Surface") { shown ->
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 18.dp, end = 18.dp, top = 18.dp, bottom = bottomInset),
                verticalArrangement = Arrangement.spacedBy(24.dp)) {
                when (shown) {
                    Surface.Widget -> WidgetGallery(app, report, accounts, active) { surface = Surface.LiveUpdate }
                    Surface.LiveUpdate -> LiveUpdateGallery(app, accounts, chip = false)
                    Surface.StatusBar -> LiveUpdateGallery(app, accounts, chip = true)
                }
            }
        }
    }
}

/** Re-checks a system setting each time Codync comes back to the front (the user may have changed it). */
@Composable internal fun <T> rememberOnResume(read: () -> T): T {
    val owner = LocalLifecycleOwner.current
    var value by remember { mutableStateOf(read()) }
    DisposableEffect(owner) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_RESUME) value = read() }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    return value
}

internal fun notificationsAllowed(context: Context) = context.getSystemService(NotificationManager::class.java).areNotificationsEnabled()

/** Android's notification prompt was answered: never asked on its own again, and reported. */
internal fun notificationsAnswered(context: Context, granted: Boolean, source: PromptSource) {
    PushPreferences.markAsked(context)
    Analytics.capture(AnalyticsEvent.NotificationsAnswered(granted, source))
}

internal fun openNotificationSettings(context: Context) = context.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
    .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))

/** The State tab's Widget page: the installed widgets and these previews share their layouts. */
@Composable private fun WidgetGallery(app: AccountContext, usage: JsonObject?, accounts: AccountStore?, active: Boolean, keepInView: () -> Unit) {
    val context = LocalContext.current
    val resumed = rememberOnResume { runCatching { CodyncWidgets.installed(context) }.getOrNull() }
    // "Check again" reads them now; coming back to the app reads them anew too.
    var checkedAgain by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(resumed) { checkedAgain = null }
    val installed = checkedAgain ?: resumed
    var asked by remember { mutableIntStateOf(0) }
    val allowed = rememberOnResume { notificationsAllowed(context) } || asked > 0 && notificationsAllowed(context)
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        notificationsAnswered(context, granted, PromptSource.Widgets); asked++; accounts?.syncPush()
    }
    var kind by rememberSaveable { mutableStateOf("usage") }
    var provider by rememberSaveable { mutableStateOf("claude") }
    var sizeChoice by rememberSaveable { mutableStateOf("medium") }
    var pinFailed by remember { mutableStateOf(false) }

    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(22.dp)).background(Palette.surface).padding(16.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Box(Modifier.size(58.dp).clip(RoundedRectangle(16.dp)).background(Palette.bubbleUser), contentAlignment = Alignment.Center) {
            CharacterAvatar("hex", "gray", 42.dp)
        }
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Codync, at a glance.", style = Type.title3.semibold, color = Palette.text)
            Text("Your bots and usage, on every surface.", style = Type.footnote, color = Palette.secondary)
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(9.dp)) {
        SectionLabel("Get set up")
        Column(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.surface)) {
            SetupRow("Connect a computer", app.computers.isNotEmpty())
            Hairline(Modifier.padding(start = 42.dp))
            SetupRow("Add a Codync widget", installed?.let { it > 0 } == true, if (installed == null) "Unable to check" else null)
            Hairline(Modifier.padding(start = 42.dp))
            SetupRow(if (!allowed && asked > 0) "Turn on notifications in Settings" else "Turn on notifications", allowed,
                action = if (allowed) null else ({
                    if (Build.VERSION.SDK_INT >= 33 && asked == 0 &&
                        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
                        permission.launch(Manifest.permission.POST_NOTIFICATIONS)
                    else openNotificationSettings(context)
                }))
        }
        if (installed == null) Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Couldn't check your widgets. You can still add one using the steps below.", style = Type.caption, color = Palette.secondary)
            SecondaryButton("Check again") { checkedAgain = runCatching { CodyncWidgets.installed(context) }.getOrNull() }
        } else Text("Steps check themselves as you finish them.", style = Type.caption2, color = Palette.tertiary)
    }

    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.weight(1f)) { SectionLabel("Choose your widget") }
            Text("Sample data", style = Type.caption2, color = Palette.secondary)
        }
        ChoicePicker(kind, listOf("usage" to "Provider usage", "limits" to "Usage limits", "bots" to "Bots", "team" to "Team"), { kind = it })
        val report = remember(usage) { usage?.let { runCatching { UsageReport.decode(it) }.getOrNull() } }
        val providers = (WidgetPreview.usage.providers + report?.providers.orEmpty()).distinctBy { it.id }
        if (kind == "usage") {
            if (providers.size <= 3) SegmentedChoice(provider, providers.map { it.id to it.name }, { provider = it })
            else ChoicePicker(provider, providers.map { it.id to it.name }, { provider = it })
        }
        // Usage limits comes small and medium, as on iOS.
        val sizes = listOf("small" to "Small", "medium" to "Medium", "large" to "Large").take(if (kind == "limits") 2 else 3)
        val size = sizeChoice.takeIf { shown -> sizes.any { it.first == shown } } ?: "medium"
        if (kind != "team") ChoicePicker(size, sizes, { picked -> sizeChoice = picked })
        Column(Modifier.fillMaxWidth().clip(RoundedRectangle(26.dp)).background(Palette.bubbleAgent).padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(18.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            val small = size == "small" || kind == "team"
            Box(Modifier.then(if (small) Modifier.width(158.dp) else Modifier.fillMaxWidth())
                .height(if (size == "large" && kind != "team") 338.dp else 158.dp)
                .clip(RoundedRectangle(22.dp)).background(Palette.surface).padding(14.dp)
                .semantics { contentDescription = "${size.replaceFirstChar(Char::uppercase)} widget preview" }) {
                when (kind) {
                    "usage" -> providers.firstOrNull { it.id == provider }?.let {
                        ProviderWidgetCard(it, if (size == "large") ProviderLayout.Large else if (size == "medium") ProviderLayout.Medium else ProviderLayout.Small)
                    }
                    "limits" -> UsageLimitsCard(WidgetPreview.usage, wide = size != "small")
                    "team" -> BotsTeamCard(teamSlots(WidgetPreview.bots, emptyList()))
                    else -> BotsWidgetCard(WidgetPreview.bots, WidgetPreview.activities, wide = size != "small", large = size == "large")
                }
            }
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(if (kind == "team") "Small widget" else "${size.replaceFirstChar(Char::uppercase)} widget", style = Type.subheadline.medium, color = Palette.text)
                Text(when (kind) {
                    "usage" -> "Small shows the tightest limit. Medium shows two windows. Large adds a summary and up to four limits."
                    "limits" -> "Every provider's limits in one widget. The tightest leads; small shows two more and medium up to four."
                    "team" -> "Four bots as characters with a dot for each one at work. Touch and hold the widget, then Edit Widget to choose who sits in each spot; empty spots show the bots that need you first."
                    else -> "See who needs you and who's still working. Large shows up to six bots."
                }, style = Type.caption, color = Palette.secondary)
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionLabel("Choose the look")
        val revision by WidgetStore.changes.collectAsState()
        val original = remember(revision) { WidgetStore.originalIcons(context) }
        SegmentedChoice(original, listOf(false to "Character", true to "Original icon"), { WidgetStore.setOriginalIcons(context, it) })
        Text("Applies to Usage in the app and in widgets.", style = Type.caption, color = Palette.secondary)
    }

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionLabel("Keep tasks in view")
        Row(Modifier.plainButton(action = keepInView), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("lock.rectangle", size = 18.dp, tint = Palette.accent)
            Text("Live updates on your Lock Screen", style = Type.subheadline, color = Palette.accent)
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionLabel("Add a widget")
        val kindToPin = when (kind) { "usage" -> WidgetKind.Provider; "limits" -> WidgetKind.Usage; "team" -> WidgetKind.Team; else -> WidgetKind.Bots }
        if (AppWidgetManager.getInstance(context).isRequestPinAppWidgetSupported)
            PrimaryButton("Add to Home Screen") { pinFailed = !CodyncWidgets.pin(context, kindToPin) }
        if (pinFailed) Text("Your launcher couldn't add it. Add it from the Home Screen instead.", style = Type.footnote, color = Palette.danger)
        WidgetSetupDemo(active)
    }
    Text("Widgets show the latest report. Open Codync for live updates and approvals.", style = Type.caption2, color = Palette.tertiary)
}

@Composable internal fun SectionLabel(title: String) =
    Text(title, Modifier.semantics { heading() }, style = Type.caption.medium, color = Palette.secondary)

@Composable private fun SetupRow(title: String, complete: Boolean, status: String? = null, action: (() -> Unit)? = null) {
    Row(Modifier.fillMaxWidth().then(if (action != null) Modifier.clickable(role = Role.Button, onClick = action) else Modifier)
        .semantics(mergeDescendants = true) { stateDescription = status ?: if (complete) "Complete" else "Not complete" }
        .padding(14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol(if (complete) "checkmark.circle.fill" else "circle", size = 18.dp, tint = if (complete) Palette.added else Palette.tertiary)
        Text(title, Modifier.weight(1f), style = Type.footnote, color = if (complete) Palette.secondary else Palette.text)
        if (status != null) Text(status, style = Type.caption2, color = Palette.tertiary)
    }
}

@Composable internal fun SettingsLink(title: String, action: () -> Unit) =
    LinkButton(title, style = Type.footnote.medium, color = Palette.accent, action = action)
