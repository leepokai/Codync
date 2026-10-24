package com.codync.android.design

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/** A button in a Codync dialog. */
class DialogAction(val title: String, val destructive: Boolean = false, val action: () -> Unit)

/**
 * A centered card over a dimmed screen with the actions and Cancel: the replacement for
 * `confirmationDialog` and `alert`. `field`: what the user types for the actions (an alert's text field).
 */
@Composable fun CodyncDialog(title: String, isPresented: Boolean, onDismiss: () -> Unit, message: String? = null,
    cancel: String? = "Cancel", field: (@Composable () -> Unit)? = null, actions: () -> List<DialogAction>) {
    val shown = remember { Animatable(0f) }
    var mounted by remember { mutableStateOf(isPresented) }
    val focus = LocalFocusManager.current
    LaunchedEffect(isPresented) {
        if (isPresented) { mounted = true; focus.clearFocus(); shown.animateTo(1f, Motion.layout()) }
        else { shown.animateTo(0f, Motion.fade()); mounted = false }
    }
    if (!mounted) return
    val spokenTitle = tr(title)
    val dismiss by rememberUpdatedState(onDismiss)
    BackHandler(isPresented && cancel != null) { dismiss() }
    Overlay {
        Box(Modifier.fillMaxSize().graphicsLayer { alpha = shown.value }.background(Color.Black.copy(alpha = 0.35f))
            .clickable(remember { MutableInteractionSource() }, null) { if (cancel != null) dismiss() }
            .imePadding().padding(24.dp), contentAlignment = Alignment.Center) {
            Column(Modifier.widthIn(max = 320.dp).fillMaxWidth()
                .graphicsLayer { val s = 0.96f + 0.04f * shown.value; scaleX = s; scaleY = s; shadowElevation = 24f; shape = RoundedRectangle(22.dp); clip = true }
                .background(Palette.bubbleAgent).clickable(remember { MutableInteractionSource() }, null) {}
                .semantics { paneTitle = spokenTitle }.padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(tr(title), style = Type.headline, color = Palette.text, textAlign = TextAlign.Center)
                    if (!message.isNullOrEmpty()) Text(tr(message), style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
                }
                field?.invoke()
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (action in actions()) DialogButton(tr(action.title), action.destructive, prominent = true) { dismiss(); action.action() }
                    if (cancel != null) DialogButton(tr(cancel), destructive = false, prominent = false) { dismiss() }
                }
            }
        }
    }
}

@Composable private fun DialogButton(title: String, destructive: Boolean, prominent: Boolean, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    Box(Modifier.fillMaxWidth().pressScale(pressed).graphicsLayer { alpha = if (pressed) 0.85f else 1f }.clip(Capsule())
        .background(if (destructive) Palette.danger else if (prominent) Palette.accentFill else Palette.bubbleUser)
        .clickable(source, null, role = Role.Button, onClick = action).padding(vertical = 12.dp), contentAlignment = Alignment.Center) {
        Text(title, style = Type.body.medium, color = if (destructive) Color.White else if (prominent) Palette.onAccent else Palette.text)
    }
}
