package com.codync.android

import android.Manifest
import android.content.pm.PackageManager
import android.os.SystemClock
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import com.codync.android.core.VoiceProvider
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.coroutines.delay

/**
 * Dictation beside send / call (iOS `Composer.micButton`): the mic and its engine menu in one light capsule; while it
 * listens or transcribes, the recording pill. A missing provider key opens Voice chat settings; a failure, its dialog.
 */
@Composable internal fun DictationButton(dictation: Dictation, state: AppState, store: AppStore, height: Dp, draft: () -> String,
    edit: ((String) -> String) -> Unit) {
    val context = LocalContext.current
    val start = { dictation.start(draft(), store.dictationService(context), edit) }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) start() else dictation.failure = "Allow microphone access in Settings to dictate."
    }
    AnimatedContent(dictation.listening || dictation.transcribing, transitionSpec = {
        (fadeIn(Motion.morph()) + scaleIn(Motion.morph(), 0.9f)) togetherWith fadeOut(Motion.morph())
    }, label = "Dictation") { recording ->
        if (recording) RecordingPill(dictation, height)
        else Row(Modifier.height(height).clip(Capsule()).background(Palette.bubbleAgent), verticalAlignment = Alignment.CenterVertically) {
            val (source, pressed) = rememberPress()
            Box(Modifier.size(height).pressScale(pressed).clickable(source, null, role = Role.Button) {
                if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) start()
                else permission.launch(Manifest.permission.RECORD_AUDIO)
            }.semantics { contentDescription = "Dictate" }, contentAlignment = Alignment.Center) {
                Symbol("mic", size = 18.dp, tint = Palette.secondary)
            }
            DictationEngineMenu(store, height)
        }
    }
    CodyncSheet(dictation.needsKey, { dictation.needsKey = false }) { VoiceChatSettings(state, store) }
    CodyncDialog("Couldn't dictate", dictation.failure != null, { dictation.failure = null }, dictation.failure, cancel = "OK") { emptyList() }
}

/** Beside the mic, in its capsule: which speech dictation uses, on-device or a provider on the user's own key. */
@Composable private fun DictationEngineMenu(store: AppStore, height: Dp) {
    val choices = remember { store.voiceChoices() }
    var engine by remember { mutableStateOf(choices.dictation) }
    DropdownMenu({
        listOf(MenuItem("On device", selected = engine == null) { engine = null; choices.dictation = null }) +
            VoiceProvider.entries.map { provider -> MenuItem(provider.title, selected = engine == provider) { engine = provider; choices.dictation = provider } }
    }, Modifier.height(height).padding(end = 6.dp).width(20.dp), label = "Dictation engine") {
        Symbol("chevron.up", Modifier.align(Alignment.Center), size = 12.dp, tint = Palette.secondary)
    }
}

/**
 * Dictation in progress, in place of the mic (iOS `RecordingPill`): stop, the time since it started and dots that
 * follow the microphone's level; a spinner once a recording waits for its transcript.
 */
@Composable private fun RecordingPill(dictation: Dictation, height: Dp) {
    Row(Modifier.height(height).clip(Capsule()).background(Palette.bubbleAgent).padding(start = 4.dp, end = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (dictation.transcribing) {
            Box(Modifier.size(height - 8.dp), contentAlignment = Alignment.Center) { Spinner(14.dp) }
            Text(tr("Transcribing…"), style = Type.subheadline, color = Palette.secondary)
        } else {
            val (source, pressed) = rememberPress()
            Box(Modifier.size(height - 8.dp).pressScale(pressed).clip(Capsule()).clickable(source, null, role = Role.Button, onClick = dictation::finish)
                .semantics { contentDescription = "Stop dictating" }, contentAlignment = Alignment.Center) {
                Symbol("stop.fill", size = 12.dp, tint = Palette.text)
            }
            var now by remember { mutableLongStateOf(SystemClock.elapsedRealtime()) }
            LaunchedEffect(dictation.startedAt) { while (true) { now = SystemClock.elapsedRealtime(); delay(500) } }
            val seconds = ((now - dictation.startedAt) / 1000).coerceAtLeast(0)
            Text("%d:%02d".format(seconds / 60, seconds % 60), style = Type.subheadline.monospacedDigit, color = Palette.text)
            LevelDots(dictation.level, speaking = false, working = false, connecting = false, color = Palette.text,
                modifier = Modifier.size(32.dp, 12.dp).semantics { contentDescription = "Listening" })
        }
    }
}
