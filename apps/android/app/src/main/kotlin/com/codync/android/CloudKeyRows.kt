package com.codync.android

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.unit.dp
import com.codync.android.core.CloudCredential
import com.codync.android.core.CloudKeySetup
import com.codync.android.core.CloudOffer
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.launch

/**
 * Codync Cloud → the key to the user's own Boat account (iOS `CloudKeyRows`): pasted once, checked and kept encrypted
 * by the cloud, and shown afterwards only by its last four. While Boat turns it down, what to fix shows under it.
 */
@Composable internal fun CloudKeyRows(offer: CloudOffer, busy: Boolean, canRemove: Boolean, store: AccountStore) {
    var replacing by remember { mutableStateOf(false) }
    val credential = offer.credential
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (credential != null && !replacing) {
            SavedKey(credential, busy, canRemove, replace = { replacing = true }, remove = store::removeCloudKey)
            credential.problem?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
        } else {
            KeyEntry(offer.keySetup, busy, replacing, cancel = { replacing = false }) { key, done ->
                store.saveCloudKey(key) { replacing = false; done() }
            }
        }
    }
}

@Composable private fun SavedKey(credential: CloudCredential, busy: Boolean, canRemove: Boolean, replace: () -> Unit, remove: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol("key.fill", size = 18.dp, tint = if (credential.problem == null) Palette.secondary else Palette.danger)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(tr("Boat API key"), style = Type.body, color = Palette.text, maxLines = 1)
            Text(credential.hint, style = Type.caption.monospaced, color = Palette.tertiary, maxLines = 1)
        }
        if (busy) Spinner()
        else {
            IconButton("Replace key", "pencil", action = replace)
            if (canRemove) IconButton("Remove key", "trash", action = remove)
        }
    }
}

/** How to make the key, then a field to paste it. [save] gets the key and what to do once it's kept. */
@Composable private fun KeyEntry(setup: CloudKeySetup?, busy: Boolean, replacing: Boolean, cancel: () -> Unit,
    save: (String, () -> Unit) -> Unit) {
    var key by remember { mutableStateOf("") }
    val submit = { if (key.isNotBlank() && !busy) save(key) { key = "" } }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (setup != null) {
            Text(tr("Make a key limited to Codync in your Boat account, then paste it here. Codync keeps it encrypted and never shows it again."),
                style = Type.footnote, color = Palette.secondary)
            Commands(setup.commands)
            WebLink("Open Boat", setup.page, Type.footnote)
        }
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PlainInput(key, { key = it }, "Boat API key", Modifier.weight(1f), secret = true, enabled = !busy)
            if (busy) Spinner() else PrimaryButton("Save", enabled = key.isNotBlank()) { submit() }
        }
        if (replacing) Text(tr("Cancel"), Modifier.plainButton(action = cancel), style = Type.footnote, color = Palette.secondary)
    }
}

@Composable private fun Commands(text: String) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var copied by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleUser).padding(start = 12.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.weight(1f)) { SelectionContainer { Text(text, style = Type.caption.monospaced, color = Palette.text) } }
        IconButton(if (copied) "Copied" else "Copy", if (copied) "checkmark" else "doc.on.doc") {
            scope.launch { clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Boat commands", text))); copied = true }
        }
    }
}
