package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.HostHealth
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** How often the section looks at the computer again while it shows: a job moves along in seconds. */
private const val REFRESH_MILLIS = 3_000L

/**
 * The computer's ComfyUI: its state, Start and Stop, and the image it is making for a bot, with Cancel.
 * Its editor opens in Codync on the computer; the phone shows and steers it, the host does the work.
 */
@Composable internal fun ComfySection(state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    var status by remember { mutableStateOf<ComfyStatus?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    fun run(block: suspend () -> Unit) = scope.launch {
        busy = true
        try { block(); error = null }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { error = e.message }
        busy = false
    }
    LaunchedEffect(state.contextId) {
        while (true) {
            try { status = store.comfyStatus(); error = null }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { error = e.message }
            delay(REFRESH_MILLIS)
        }
    }
    CardSection("ComfyUI", "Make images on the computer. Its editor opens in Codync on the computer.") {
        row {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val current = status
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Symbol("comfyui.logo", size = 20.dp, tint = Palette.text)
                    Text(
                        when {
                            current == null -> tr("Checking {name}…").replace("{name}", "ComfyUI")
                            !current.bundled -> tr("This build of Codync does not include ComfyUI.")
                            else -> tr(current.stateLabel).replace("{name}", "ComfyUI")
                        },
                        style = Type.body, color = Palette.text, modifier = Modifier.weight(1f),
                    )
                }
                current?.notice?.let { Text(tr(it), style = Type.caption, color = Palette.secondary) }
                current?.error?.let { Text(it, style = Type.caption, color = Palette.danger) }
                if (current != null && current.bundled) {
                    when {
                        current.installPhase == "downloading" -> Text(
                            tr("Downloading {name}… {done} of {total}").replace("{name}", "ComfyUI")
                                .replace("{done}", HostHealth.bytesText(current.installDone))
                                .replace("{total}", HostHealth.bytesText(current.installTotal)),
                            style = Type.caption, color = Palette.secondary,
                        )
                        current.installPhase == "unpacking" ->
                            Text(tr("Unpacking {name}…").replace("{name}", "ComfyUI"), style = Type.caption, color = Palette.secondary)
                        !current.installed -> Text(
                            tr("{name} downloads about {size} the first time it starts.").replace("{name}", "ComfyUI")
                                .replace("{size}", HostHealth.bytesText(current.downloadBytes)),
                            style = Type.caption, color = Palette.secondary,
                        )
                    }
                    current.installError?.takeIf { current.installPhase == "failed" }?.let {
                        Text(it, style = Type.caption, color = Palette.danger)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        if (current.active) {
                            SecondaryButton(tr("Stop"), enabled = !busy) { run { status = store.comfyStop() } }
                        } else {
                            SecondaryButton(tr(if (current.installed) "Start" else "Download and start"),
                                enabled = !busy && !current.installing) { run { status = store.comfyStart() } }
                        }
                    }
                }
                current?.jobLabel?.let { label ->
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(tr(label), style = Type.caption, color = Palette.secondary, modifier = Modifier.weight(1f))
                        if (current.jobActive) {
                            SecondaryButton(tr("Cancel"), enabled = !busy) { run { store.comfyCancel(); status = store.comfyStatus() } }
                        }
                    }
                }
                error?.let { Text(it, style = Type.caption, color = Palette.danger) }
            }
        }
    }
}
