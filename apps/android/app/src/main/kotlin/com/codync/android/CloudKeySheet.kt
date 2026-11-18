package com.codync.android

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
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
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.dp
import com.codync.android.core.CloudCredential
import com.codync.android.core.CloudOffer
import com.codync.android.core.CloudProvider
import com.codync.android.core.CloudProviderOffer
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.launch

/** "Boat API key": a provider's key sheet, its menu items, and its field. */
@Composable internal fun keyTitle(providerName: String) = tr("{provider} API key").replace("{provider}", providerName)

/** One provider's key (iOS `CloudKeySheet`), from a provider row's or a cloud computer's `⋯`: [CloudKeyForm]. Saving one
 * closes the sheet. */
@Composable internal fun CloudKeySheet(provider: String, offer: CloudOffer, state: AccountContext, store: AccountStore) {
    DisposableEffect(store) {
        store.cloudSheetShown(true)
        onDispose { store.cloudSheetShown(false) }
    }
    val p = offer.providers.firstOrNull { it.provider == provider } ?: return
    val dismiss = LocalDismissModal.current
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(keyTitle(p.name))
        CardForm { CloudKeyForm(p, offer, state, store, dismiss) }
    }
}

/**
 * One provider's key (iOS `CloudKeyForm`), in its own sheet and as New cloud computer's key step: how to make one on the
 * provider's site (numbered, then its page), the key to the user's own account there (pasted once, checked and kept
 * encrypted by the cloud, shown afterwards only by its last four, with what to fix while the provider turns it down),
 * its free plan, and the command line way, folded away. [saved] runs once a key is kept.
 */
@Composable internal fun CloudKeyForm(provider: CloudProvider, offer: CloudOffer, state: AccountContext, store: AccountStore, saved: () -> Unit) {
    // Above the key: the last step says to paste it below. An older cloud sends no steps, only the page.
    val steps = provider.setup.steps
    CardSection {
        if (steps.isNotEmpty()) row {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                for ((index, step) in steps.withIndex()) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("${index + 1}.", Modifier.alignByBaseline(), style = Type.body.monospacedDigit, color = Palette.tertiary)
                    Text(tr(step), Modifier.alignByBaseline(), style = Type.body, color = Palette.text)
                }
            }
        }
        row { WebLink(tr("Open {provider}").replace("{provider}", provider.name), provider.setup.page) }
    }
    CardSection(footer = "Your cloud computer runs on your own account. Keys stay encrypted; only their last four show.") {
        row {
            KeyRow(provider, cloudKeyAction(provider.provider) in state.busy, offer.canRemoveKey(provider.provider),
                state.cloudErrorAt(CloudErrorPlace.Key(provider.provider)), store, saved)
        }
        provider.offer?.let { row { OfferRow(provider.provider, it) } }
        row { CommandLine(provider.setup.commands) }
    }
}

/** The provider's key: its last four with replace and remove, or a field to paste it; [error] when saving or removing
 * it failed, under the key. [saved] runs once one is kept. */
@Composable private fun KeyRow(provider: CloudProvider, busy: Boolean, canRemove: Boolean, error: String?, store: AccountStore, saved: () -> Unit) {
    var replacing by remember { mutableStateOf(false) }
    val credential = provider.credential
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (credential != null && !replacing) {
            SavedKey(credential, busy, canRemove, replace = { replacing = true }) { store.removeCloudKey(provider.provider) }
            credential.problem?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
        } else {
            KeyEntry(provider, busy, replacing, cancel = { replacing = false }) { key, done ->
                store.saveCloudKey(provider.provider, key) { replacing = false; done(); saved() }
            }
        }
        error?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
    }
}

@Composable private fun SavedKey(credential: CloudCredential, busy: Boolean, canRemove: Boolean, replace: () -> Unit, remove: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol("key.fill", size = 18.dp, tint = if (credential.problem == null) Palette.secondary else Palette.danger)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(tr("API key"), style = Type.body, color = Palette.text, maxLines = 1)
            Text(credential.hint, style = Type.caption.monospaced, color = Palette.tertiary, maxLines = 1)
        }
        if (busy) Spinner()
        else {
            IconButton("Replace key", "pencil", action = replace)
            if (canRemove) IconButton("Remove key", "trash", action = remove)
        }
    }
}

/** A field to paste the key. [save] gets the key and what to do once it's kept. */
@Composable private fun KeyEntry(provider: CloudProvider, busy: Boolean, replacing: Boolean, cancel: () -> Unit,
    save: (String, () -> Unit) -> Unit) {
    var key by remember { mutableStateOf("") }
    val focus = LocalFocusManager.current
    val label = keyTitle(provider.name)
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.weight(1f)) {
            PlainInput(key, { key = it }, provider.keyPrompt, secret = true, enabled = !busy, label = label)
        }
        if (replacing) Text(tr("Cancel"), Modifier.plainButton(action = cancel), style = Type.body, color = Palette.secondary)
        if (busy) Spinner() else PrimaryButton("Save", enabled = key.isNotBlank()) {
            // Drops the keyboard first: left up, it covers why the provider turned the key down.
            focus.clearFocus()
            save(key) { key = "" }
        }
    }
}

/** The provider's own free plan: what it includes, and its page. */
@Composable private fun OfferRow(provider: String, offer: CloudProviderOffer) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(tr(offer.detail), style = Type.footnote, color = Palette.secondary)
        WebLink(offer.label, offer.url, Type.footnote) { Analytics.capture(AnalyticsEvent.CloudOfferOpened(provider)) }
    }
}

/** The cloud's commands that make the key, folded away. */
@Composable private fun CommandLine(commands: String) {
    var open by remember { mutableStateOf(false) }
    Disclosure(open, { open = !open }, label = { Text(tr("Use the command line instead"), style = Type.body, color = Palette.text) }) {
        Commands(commands)
    }
}

/** One command a line, scrolled sideways rather than wrapped: wrapping breaks them into commands that don't exist. */
@Composable private fun Commands(text: String) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var copied by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleUser).padding(start = 12.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.weight(1f).horizontalScroll(rememberScrollState())) {
            SelectionContainer { Text(text, style = Type.caption.monospaced, color = Palette.text, softWrap = false) }
        }
        IconButton(if (copied) "Copied" else "Copy", if (copied) "checkmark" else "doc.on.doc") {
            scope.launch { clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Key commands", text))); copied = true }
        }
    }
}
