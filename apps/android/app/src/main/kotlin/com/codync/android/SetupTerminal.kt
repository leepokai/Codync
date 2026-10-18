package com.codync.android

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Intent
import android.os.Build
import android.os.PersistableBundle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentAuthMethod
import com.codync.android.core.AgentBackend
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import java.util.UUID

/**
 * Installing or signing in to an agent, or adding Node.js, live (iOS `SetupTerminalView`): the
 * command runs in a terminal on the computer and this screen is that terminal (links open here).
 * `backend` is null only for the `node` step, which belongs to the computer.
 */
@Composable internal fun SetupTerminal(backend: AgentBackend?, step: String, method: AgentAuthMethod?, state: AppState, store: AppStore, back: () -> Unit) {
    val owner = rememberSaveable { UUID.randomUUID().toString() }
    val session = remember(owner) { store.setupTerminal(owner, backend?.id.orEmpty(), step, method?.id) }
    val status by session.state.collectAsState()
    var renderer by remember { mutableStateOf<TerminalRenderer?>(null) }
    var renderError by remember { mutableStateOf(false) }
    val activity = LocalActivity.current
    val context = LocalContext.current
    val clipboard = remember(context) { context.getSystemService(ClipboardManager::class.java) }
    // Leaving the screen ends whatever is still running; rotation keeps it.
    val leave = { store.closeSetupTerminal(owner); back() }
    DisposableEffect(session) {
        onDispose {
            renderer?.let(session::detach)
            if (activity?.isChangingConfigurations != true) store.closeSetupTerminal(owner)
        }
    }
    BackHandler(onBack = leave)
    val name = backend?.name ?: "the agent"
    val title = when (step) { "node" -> "Add Node.js"; "install" -> "Install $name"; else -> method?.name ?: "Sign in to $name" }
    val live = status.connected && status.exitCode == null && status.error == null
    Column(Modifier.fillMaxSize().background(Palette.background).imePadding()) {
        ScreenHeader({ BackButton(leave) }, { Text(title, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }) {
            IconButton("Copy selection", "doc.on.doc", enabled = renderer != null) { renderer?.copySelection() }
        }
        Box(Modifier.weight(1f).fillMaxWidth().padding(start = 10.dp, end = 10.dp, top = 6.dp).clip(RoundedRectangle(12.dp))) {
            if (!renderError) TerminalSurface(Modifier.fillMaxSize(), { target -> renderer = target; session.attach(target) }, session::type, session::resized,
                { renderError = true; store.closeSetupTerminal(owner); renderer = null },
                openLink = { uri -> runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, uri)) } },
                copy = { text ->
                    val clip = ClipData.newPlainText("Agent setup terminal", text)
                    if (Build.VERSION.SDK_INT >= 33) clip.description.extras = PersistableBundle().apply { putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true) }
                    clipboard.setPrimaryClip(clip)
                }, created = { renderer = it })
        }
        // The keys a phone keyboard lacks.
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 10.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for ((label, bytes) in listOf("Ctrl+C" to byteArrayOf(3), "Tab" to byteArrayOf(9), "Esc" to byteArrayOf(27))) {
                Key(label, live) { session.type(bytes) }
            }
            Key("Keyboard", renderer != null) { renderer?.focus(showKeyboard = true) }
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val code = status.exitCode
            val error = status.error
            when {
                renderError -> Text("The terminal couldn't load. Update Android System WebView, then try again.", Modifier.weight(1f),
                    style = Type.footnote, color = Palette.danger, maxLines = 2, overflow = TextOverflow.Ellipsis)
                error != null -> Text(error, Modifier.weight(1f), style = Type.footnote, color = Palette.danger, maxLines = 2, overflow = TextOverflow.Ellipsis)
                code != null -> {
                    Symbol(if (code == 0) "checkmark.circle.fill" else "exclamationmark.circle.fill", size = 16.dp, tint = if (code == 0) Palette.added else Palette.warning)
                    Text(if (code == 0) "Finished." else "Ended with an error ($code).", Modifier.weight(1f), style = Type.footnote, color = Palette.secondary)
                }
                else -> {
                    ThinkingOrb(size = 14.dp, color = Palette.secondary)
                    Text(if (status.starting) "Starting setup on ${state.hostName}…" else if (!status.connected) "Reconnecting to the setup terminal…"
                        else "Running on ${state.hostName}. Links open on this device.", Modifier.weight(1f), style = Type.footnote, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
            }
            if (code != null) PrimaryButton("Done", action = leave)
        }
    }
}

@Composable private fun Key(label: String, enabled: Boolean, action: () -> Unit) {
    Text(label, Modifier.clip(Capsule()).background(Palette.bubbleAgent).plainButton(enabled = enabled, action = action)
        .padding(horizontal = 12.dp, vertical = 6.dp), style = Type.footnote.medium, color = if (enabled) Palette.text else Palette.tertiary)
}
