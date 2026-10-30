package com.codync.android

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*

internal val LocalAgentIconOverrides = staticCompositionLocalOf<Map<String, CustomIcon>> { emptyMap() }

internal fun agentIconOverrides(state: AppState): Map<String, CustomIcon> = buildMap {
    for (agent in agentBackends(state)) state.icons["agent:${agent.id}"]?.let { icon ->
        put(agent.id, icon)
        agent.registry?.let { put(it, icon) }
    }
}

@Composable internal fun CustomIconEditor(target: String, state: AppState, store: AppStore) {
    if (state.hello?.containsKey("icons") != true) return
    var needsSignIn by remember { mutableStateOf(false) }
    val requireAccount: (() -> Unit) -> Unit = { action -> if (state.account.userId != null) action() else needsSignIn = true }
    var url by remember(target) { mutableStateOf("") }
    var showURL by remember(target) { mutableStateOf(false) }
    val icon = state.icons[target]
    val busy = "icon:$target" in state.busy
    val enabled = !busy && state.account.ready && "session" !in state.busy && state.actualConnection is LinkState.Ready
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { requireAccount { store.setIcon(target, file = it) } }
    }
    CodyncDialog("Sign in required", needsSignIn, { needsSignIn = false }, message = "Sign in to use custom images.") {
        listOf(
            DialogAction("Continue with Apple") { store.signIn(SignInProvider.Apple) },
            DialogAction("Continue with Google") { store.signIn(SignInProvider.Google) },
        )
    }
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(tr("Custom icon"), style = Type.subheadline.semibold, color = Palette.text)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            if (icon != null) StaticCustomIcon(icon.dataUrl, icon.tinted, 40.dp)
            SecondaryButton("Upload image", enabled = enabled) { requireAccount { picker.launch(arrayOf("image/png", "image/jpeg", "image/webp", "image/svg+xml")) } }
            if (busy) ThinkingOrb(size = 18.dp)
        }
        if (icon != null) SecondaryButton("Use default", enabled = enabled) { store.setIcon(target, remove = true) }
        SecondaryButton("Paste URL", enabled = enabled) { requireAccount { showURL = !showURL } }
        AnimatedVisibility(showURL, enter = fadeIn(), exit = fadeOut()) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(tr("Image URL"), style = Type.caption, color = Palette.secondary)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Box(Modifier.weight(1f)) { FieldBox(url, { if (enabled) url = it }, "https://…", capitalize = false) }
                    IconButton("Use URL", "arrow.up", enabled = enabled && url.isNotBlank()) { requireAccount { store.setIcon(target, url = url.trim()) } }
                }
            }
        }
        if (icon != null) SwitchRow("Tint icon", isOn = icon.tinted, onChange = { store.setIcon(target, tinted = it) }, enabled = enabled)
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(tr("PNG, JPEG, WebP up to 5 MiB; SVG up to 256 KiB."), style = Type.caption, color = Palette.secondary, textAlign = TextAlign.Center)
            Text(tr("Saved immediately. Custom images stay still."), style = Type.caption, color = Palette.secondary, textAlign = TextAlign.Center)
        }
        if (state.error != null) Text(tr(state.error), style = Type.caption, color = Palette.danger)
    }
}
