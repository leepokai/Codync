package com.codync.android

import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay

/** A sample task's phase, as iOS `BotActivityPresentation.Phase`. */
internal enum class TaskPhase(val title: String) {
    Sending("Sending"), Queued("Waiting for your computer"), Working("Working"), NeedsInput("Needs you"),
    Completed("Done"), Failed("Failed"), Stale("Update delayed");

    val live get() = this == Working || this == NeedsInput
}

@Composable internal fun TaskPhase.tint(): Color = when (this) {
    TaskPhase.NeedsInput -> Palette.attention
    TaskPhase.Failed -> Palette.danger
    TaskPhase.Completed -> Palette.added
    TaskPhase.Working -> Palette.text
    else -> Palette.secondary
}

/** Live updates (on Android 16, promoted ongoing notifications) are what iOS calls Live Activities. */
internal object LiveUpdates {
    /** Promoted notifications (Live updates, the status bar chip) need Android 16 QPR1. */
    val supported: Boolean get() = Build.VERSION.SDK_INT >= 36 && Build.VERSION.SDK_INT_FULL >= Build.VERSION_CODES_FULL.BAKLAVA_1

    /** Android lets the user turn promotion off per app. */
    fun promoted(context: Context): Boolean = supported &&
        context.getSystemService(NotificationManager::class.java).canPostPromotedNotifications()

    fun openSettings(context: Context) {
        val action = if (supported) Settings.ACTION_APP_NOTIFICATION_PROMOTION_SETTINGS else Settings.ACTION_APP_NOTIFICATION_SETTINGS
        context.startActivity(Intent(action).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}

/**
 * The Live update and Status bar pages: one switch (the chip shows the same Live update), a
 * sample task in any phase, and what each presentation shows.
 */
@Composable internal fun LiveUpdateGallery(state: AppState, store: AppStore?, chip: Boolean) {
    val context = LocalContext.current
    val notifications = rememberOnResume { notificationsAllowed(context) }
    val promoted = rememberOnResume { LiveUpdates.promoted(context) }
    var phase by rememberSaveable { mutableStateOf(TaskPhase.Working) }
    val started = remember { System.currentTimeMillis() - 154_000 }
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(20.dp)).background(Palette.surface).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        CodyncSwitch(state.tasks, { store?.taskNotifications(it) }) {
            Text(if (chip) "Show in the status bar" else "Show Live updates", style = Type.subheadline.medium, color = Palette.text)
        }
        Text(if (chip) "The status bar chip shows the same Live update as the Lock Screen, so this switch turns off both."
            else "Starts when you send a task from this phone. Follow its progress, then open the conversation when your bot needs you.",
            style = Type.footnote, color = Palette.secondary)
        when {
            !notifications -> SettingsNote("Notifications are turned off in Android settings.") { openNotificationSettings(context) }
            !LiveUpdates.supported -> Text("On this Android version a task shows as an ongoing notification. Live updates and the status bar chip need Android 16 QPR1.",
                style = Type.footnote, color = Palette.secondary)
            !promoted -> SettingsNote("Live updates are turned off for Codync in Android settings.") { LiveUpdates.openSettings(context) }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Preview", Modifier.weight(1f), style = Type.subheadline.medium, color = Palette.text)
            Text("Sample task", style = Type.caption, color = Palette.secondary)
        }
        ChoicePicker(phase, listOf(TaskPhase.Sending to "Sending", TaskPhase.Queued to "Waiting for computer", TaskPhase.Working to "Working",
            TaskPhase.NeedsInput to "Needs you", TaskPhase.Completed to "Done", TaskPhase.Failed to "Error", TaskPhase.Stale to "Update delayed"), { phase = it })
        Box(Modifier.fillMaxWidth().heightIn(min = 90.dp).clip(RoundedRectangle(28.dp)).background(Palette.bubbleAgent).padding(12.dp),
            contentAlignment = Alignment.Center) {
            if (chip) StatusBarPreview(phase, started) else LiveUpdatePreview(phase, started)
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        if (chip) {
            Explanation("Chip", "The bot and its running time beside the clock, in the state's color. It becomes a check or a warning when the task ends.")
            Explanation("Tap", "Tap the chip to see the task's Live update; tap that to open the conversation.")
        } else {
            Explanation("Lock Screen and notifications", "The bot in its mood, its current step in the state's color, the running time and progress. When the bot needs you, Review opens the request.")
        }
    }
    Text("Android chooses how Live updates appear based on your device and other tasks. These previews don't start one. Approvals are handled inside Codync.",
        style = Type.footnote, color = Palette.secondary)
}

@Composable private fun SettingsNote(text: String, open: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(text, style = Type.footnote, color = Palette.secondary)
        LinkButton("Open Android settings", style = Type.footnote.medium, color = Palette.accent, action = open)
    }
}

@Composable private fun Explanation(title: String, detail: String) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SectionTitle(title)
        Text(detail, style = Type.footnote, color = Palette.secondary)
    }
}

@Composable private fun SectionTitle(title: String) = Text(title, style = Type.subheadline.medium, color = Palette.text)

/** The step (or state) in its color, then the running time while the task is live. */
@Composable private fun Caption(phase: TaskPhase, started: Long) {
    val caption = if (phase.live) (if (phase == TaskPhase.NeedsInput) "Review the proposed changes." else "Running the test suite.") else phase.title
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(caption, Modifier.weight(1f, fill = false), style = Type.system(12f), color = if (phase == TaskPhase.Working) Palette.secondary else phase.tint(), maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (phase.live) {
            Text("·", style = Type.system(12f), color = Palette.tertiary)
            Text(elapsed(started), style = Type.system(12f).monospacedDigit, color = Palette.secondary)
        }
    }
}

@Composable private fun elapsed(started: Long): String {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(started) { while (true) { now = System.currentTimeMillis(); delay(1000) } }
    val seconds = (now - started) / 1000
    return "%d:%02d".format(seconds / 60, seconds % 60)
}

/** The end mark: a thinking orb while it runs, a check or a warning once it ends. */
@Composable private fun PhaseMark(phase: TaskPhase, size: androidx.compose.ui.unit.Dp) {
    when (phase) {
        TaskPhase.Completed -> Symbol("checkmark", size = size, tint = phase.tint())
        TaskPhase.Failed -> Symbol("exclamationmark.triangle.fill", size = size, tint = phase.tint())
        TaskPhase.Queued -> Symbol("desktopcomputer", size = size, tint = phase.tint())
        else -> ThinkingOrb(when (phase) { TaskPhase.Working -> ThinkingOrbState.Working; TaskPhase.NeedsInput -> ThinkingOrbState.Listening
            else -> ThinkingOrbState.Connecting }, size, phase.tint())
    }
}

private val TaskPhase.mood get() = when (this) { TaskPhase.Working -> "working"; TaskPhase.NeedsInput -> "needsInput"; else -> "idle" }

/** The Lock Screen Live update: app and time, the bot in its mood, its step, a progress line, Review when it needs you. */
@Composable private fun LiveUpdatePreview(phase: TaskPhase, started: Long) {
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(22.dp)).background(Palette.surface).padding(14.dp).animateContentSize(Motion.layoutSize),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Box(Modifier.size(18.dp).clip(Capsule()).background(Palette.accentFill), contentAlignment = Alignment.Center) {
                androidx.compose.foundation.Image(painterResource(R.drawable.ic_notification), null, Modifier.size(12.dp),
                    colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(Palette.onAccent))
            }
            Text(androidx.compose.ui.res.stringResource(R.string.app_name), style = Type.caption.medium, color = Palette.secondary)
            Text("· now", style = Type.caption, color = Palette.tertiary)
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            CharacterAvatar("blob", "green", 40.dp, phase.mood)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(tr("Reviewer"), style = Type.system(14f).semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Caption(phase, started)
            }
            PhaseMark(phase, 20.dp)
        }
        ProgressLine(phase)
        AnimatedVisibility(phase == TaskPhase.NeedsInput, enter = fadeIn(Motion.fade()) + slideInVertically { it / 2 }, exit = fadeOut(Motion.fade())) {
            Row(Modifier.fillMaxWidth().height(34.dp).clip(Capsule()).background(phase.tint()), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.Center) {
                Symbol("hand.raised.fill", size = 15.dp, tint = Palette.onAccent)
                Spacer(Modifier.width(6.dp))
                Text("Review", style = Type.system(13f).semibold, color = Palette.onAccent)
            }
        }
    }
}

/** Android's Live update progress: segments that fill as the task moves, in the state's color. */
@Composable private fun ProgressLine(phase: TaskPhase) {
    val tint = phase.tint()
    val filled = when (phase) { TaskPhase.Sending, TaskPhase.Queued -> 1; TaskPhase.Working, TaskPhase.Stale -> 2; TaskPhase.NeedsInput -> 3; else -> 4 }
    val label = tr("Progress, step $filled of 4")
    Canvas(Modifier.fillMaxWidth().height(6.dp).semantics { contentDescription = label }) {
        val gap = 3.dp.toPx(); val width = (size.width - gap * 3) / 4
        for (i in 0 until 4) drawRoundRect(if (i < filled) tint else tint.copy(alpha = 0.16f), Offset(i * (width + gap), 0f),
            Size(width, size.height), CornerRadius(size.height / 2))
    }
}

/** The status bar with the task's chip beside the clock (Android 16's promoted notification chip). */
@Composable private fun StatusBarPreview(phase: TaskPhase, started: Long) {
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Color.Black).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically) {
        Text("9:41", style = Type.system(14f).semibold.monospacedDigit, color = Color.White)
        Spacer(Modifier.width(8.dp))
        Row(Modifier.clip(Capsule()).background(if (phase == TaskPhase.NeedsInput || phase == TaskPhase.Failed) phase.tint() else Color(0xFF2C2C2C))
            .padding(start = 7.dp, end = 10.dp, top = 4.dp, bottom = 4.dp).animateContentSize(Motion.layoutSize),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            // Android draws the app's small icon in the chip.
            androidx.compose.foundation.Image(painterResource(R.drawable.ic_notification), null, Modifier.size(16.dp),
                colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(Color.White))
            Text(when { phase.live && phase != TaskPhase.NeedsInput -> elapsed(started); phase == TaskPhase.NeedsInput -> "Needs you"
                phase == TaskPhase.Completed -> "Done"; phase == TaskPhase.Failed -> "Failed"; else -> phase.title },
                style = Type.system(12f).semibold.monospacedDigit, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Spacer(Modifier.weight(1f))
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            Symbol("wifi", size = 15.dp, tint = Color.White)
            Symbol("battery.75percent", size = 18.dp, tint = Color.White)
        }
    }
}
