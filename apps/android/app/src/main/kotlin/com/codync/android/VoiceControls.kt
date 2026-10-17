package com.codync.android

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import kotlin.math.sin
import androidx.core.content.ContextCompat
import com.codync.android.core.*
import com.codync.android.design.*

/** Starts a call with `botId`, asking for the microphone first; `denied` reports a refusal. */
@Composable internal fun rememberStartVoice(botId: String, state: AppState, store: AppStore, denied: () -> Unit): () -> Unit {
    val context = LocalContext.current
    var requested by remember { mutableStateOf<Pair<String, Int>?>(null) }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        val owner = requested; requested = null
        if (owner != null && store.state.value.generation == owner.second && store.state.value.selectedBot == owner.first) {
            if (granted) store.startVoice(owner.first, owner.second) else denied()
        }
    }
    return {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) store.startVoice(botId, state.generation)
        else { requested = botId to state.generation; permission.launch(Manifest.permission.RECORD_AUDIO) }
    }
}

/** Opens this app's settings, where the microphone can be allowed. */
internal fun openAppSettings(context: android.content.Context) =
    context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}")))

/**
 * A voice call with a bot (iOS `CallView`): a floating glass bar over the chat, which stays readable
 * underneath. What you say goes out as ordinary messages; its final replies are read aloud.
 */
@Composable internal fun CallBar(call: VoiceCallViewState, state: AppState, store: AppStore) {
    val voice = call.state
    val bot = state.bots.firstOrNull { it.id == call.botId }
    val color = bot?.let { AvatarPalette.tint(it.avatarColor) } ?: Palette.accent
    var settings by remember { mutableStateOf(false) }
    val speaking = voice.phase == VoicePhase.Speaking
    LaunchedEffect(call.id) { Analytics.capture(AnalyticsEvent.VoiceCallStarted) }
    Column(Modifier.padding(horizontal = 12.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth().glass(Capsule()).padding(start = 14.dp, end = 7.dp, top = 7.dp, bottom = 7.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            // The bot; tapping it while it talks cuts the reply short, otherwise it opens the call's chat.
            val pulse by rememberInfiniteTransition(label = "Speaking").animateFloat(1f, 1.1f,
                infiniteRepeatable(tween(600), RepeatMode.Reverse), label = "Pulse")
            Box(Modifier.graphicsLayer { val s = if (speaking) pulse else 1f; scaleX = s; scaleY = s }
                .plainButton { if (speaking) store.interruptVoice() else if (state.selectedBot != call.botId) store.openBot(call.botId) }
                .semantics { contentDescription = if (speaking) "Interrupt" else bot?.name.orEmpty() }) {
                if (bot != null) BotAvatar(bot, state.bots, 34.dp)
            }
            CallLevelDots(voice.level, speaking, bot?.status == "working", color,
                Modifier.weight(1f).height(20.dp).semantics { contentDescription = statusText(voice, bot?.status == "working") })
            RoundButton("Call settings", "gearshape") { settings = true }
            RoundButton(if (voice.muted) "Unmute" else "Mute", if (voice.muted) "mic.slash" else "mic", on = voice.muted, action = store::muteVoice)
            Box(Modifier.size(46.dp).clip(Capsule()).background(Palette.danger).plainButton { store.endVoice() }
                .semantics { contentDescription = "End call" }, contentAlignment = Alignment.Center) {
                Symbol("xmark", size = 18.dp, tint = Color.White)
            }
        }
        val failure = voice.error ?: if (voice.phase == VoicePhase.Failed) "Voice audio unavailable" else null
        when {
            failure != null -> CallLine(failure, Palette.danger)
            voice.phase == VoicePhase.Interrupted -> CallLine("Audio paused by another app. Tap to resume.", Palette.secondary, store::resumeVoice)
        }
    }
    CodyncSheet(settings, { settings = false }) { VoiceChatSettings(store) }
}

@Composable private fun CallLine(text: String, color: Color, action: (() -> Unit)? = null) {
    Text(text, Modifier.glass(Capsule()).then(if (action != null) Modifier.plainButton(action = action) else Modifier)
        .padding(horizontal = 12.dp, vertical = 8.dp), style = Type.footnote, color = color, textAlign = TextAlign.Center)
}

@Composable private fun RoundButton(label: String, systemImage: String, on: Boolean = false, action: () -> Unit) {
    Box(Modifier.size(46.dp).clip(Capsule()).background(if (on) Palette.accentFill else Palette.background).plainButton(action = action)
        .semantics { contentDescription = label }, contentAlignment = Alignment.Center) {
        Symbol(systemImage, size = 20.dp, tint = if (on) Palette.onAccent else Palette.text)
    }
}

private fun statusText(voice: VoiceState, working: Boolean) = when (voice.phase) {
    VoicePhase.Starting -> "Connecting"
    VoicePhase.Speaking -> "Speaking"
    VoicePhase.Failed -> "Can't listen"
    VoicePhase.Interrupted -> "Paused"
    VoicePhase.Ended -> "Ending"
    VoicePhase.Listening -> if (voice.muted) "Muted" else if (working) "Working, listening" else "Listening"
}

/**
 * The dotted line in the call bar: it follows your voice while listening, ripples while the bot
 * talks, breathes while it works, and rests as faint dots otherwise.
 */
@Composable private fun CallLevelDots(level: Float, speaking: Boolean, working: Boolean, color: Color, modifier: Modifier) {
    val still = reduceMotion()
    var time by remember { mutableFloatStateOf(0f) }
    if (!still) LaunchedEffect(Unit) { while (true) withFrameNanos { time = it / 1e9f } }
    val accent = Palette.accent
    Canvas(modifier) {
        val spacing = 8.dp.toPx()
        val width = 2.5.dp.toPx()
        val count = maxOf(1, (size.width / spacing).toInt())
        for (i in 0 until count) {
            val wave = (sin(i * 0.55f - time * 6f) + 1) / 2
            val amount = when {
                speaking -> 0.5f + 0.5f * wave
                level > 0.05f -> maxOf(0.2f, level) * (0.45f + 0.55f * wave)
                working -> 0.25f * ((sin(time * 2.4f) + 1) / 2)
                else -> 0.08f
            }
            val height = 4.dp.toPx() + amount * (size.height - 4.dp.toPx())
            drawRoundRect((if (speaking) accent else color).copy(alpha = 0.5f + 0.5f * amount),
                Offset(i * spacing + spacing / 2 - width / 2, (size.height - height) / 2), Size(width, height), CornerRadius(width / 2))
        }
    }
}

/** Voice chat settings (iOS `VoiceChatSettingsView`): Android speech on this phone, its pause and reading speed. */
@Composable internal fun VoiceChatSettings(store: AppStore) {
    val context = LocalContext.current
    var settings by remember { mutableStateOf(VoicePreferences.load(context)) }
    fun change(next: VoiceSettings) { settings = next; store.voiceSettings(next) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Voice chat")
        CardForm {
            CardSection("Voice", footer = "Android speech, on this phone. Only the text reaches the computer.") {
                row { ValueRow("Engine", "On device") }
                row {
                    ValueRow("Reading speed") {
                        ChoicePicker(settings.rate, listOf(.84f to "Slower", 1f to "Normal", 1.12f to "Faster"), { change(settings.copy(rate = it)) },
                            label = "Reading speed")
                    }
                }
                row {
                    ValueRow("Send after a pause of", detail = "Longer leaves time to think mid-sentence.") {
                        ChoicePicker(settings.pauseMillis, listOf(1_000L to "1 s", 1_500L to "1.5 s", 2_500L to "2.5 s"),
                            { change(settings.copy(pauseMillis = it)) }, label = "Send after a pause of")
                    }
                }
            }
        }
    }
}
