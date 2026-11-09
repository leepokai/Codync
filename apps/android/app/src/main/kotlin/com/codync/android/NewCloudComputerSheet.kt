package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.CloudOffer
import com.codync.android.core.CloudProvider
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/**
 * Computers → Codync Cloud → "+" (iOS `NewCloudComputerSheet`): its name (empty, the suggested one), then the provider it
 * runs on. Every provider the cloud offers is listed with its logo; one without a usable key opens that provider's key
 * ([openKey]) and is picked once it has one, and one with a key opens it, or removes it, from its row's `⋯`. Create closes
 * the sheet once the computer is made; a refusal shows here, and at the account's limit why it's off.
 */
@Composable internal fun NewCloudComputerSheet(offer: CloudOffer, state: AccountContext, store: AccountStore, openKey: (String) -> Unit) {
    DisposableEffect(store) {
        store.cloudSheetShown(true)
        onDispose { store.cloudSheetShown(false) }
    }
    val dismiss = LocalDismissModal.current
    var name by remember { mutableStateOf("") }
    var picked by remember { mutableStateOf<String?>(null) }
    val selected = offer.chosenProvider(picked)
    val suggested = offer.suggestedName(tr("Cloud computer"), tr("Cloud computer {n}"))
    val creating = CLOUD_CREATE in state.busy
    // One action at a time, as on iOS: a key being saved or removed holds Create and the other rows' `⋯`.
    val busy = creating || offer.providers.any { cloudKeyAction(it.provider) in state.busy }
    val full = offer.machines.size >= offer.limit
    val create = { if (selected != null && !busy && !full) store.createCloudComputer(selected, name.trim().ifEmpty { suggested }, dismiss) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("New cloud computer") {
            if (creating) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Create", "checkmark", enabled = selected != null && !busy && !full) { create() }
        }
        CardForm {
            CardSection("Name") {
                row {
                    val label = tr("Name your cloud computer")
                    PlainTextField(name, { name = it.take(CLOUD_NAME_LIMIT) }, Modifier.fillMaxWidth().semantics { contentDescription = label }, suggested,
                        singleLine = true, keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
                        actions = KeyboardActions(onDone = { create() }))
                }
            }
            CardSection("Provider") {
                for (provider in offer.providers) row {
                    val keyLabel = keyTitle(provider.name)
                    val menu = {
                        buildList {
                            add(MenuItem(keyLabel, icon = "key.fill", verbatim = true) { openKey(provider.provider) })
                            if (offer.canRemoveKey(provider.provider)) {
                                add(MenuItem("Remove key", icon = "trash", destructive = true) { store.removeCloudKey(provider.provider) })
                            }
                        }
                    }
                    ProviderRow(provider, provider.provider == selected, cloudKeyAction(provider.provider) in state.busy,
                        menuEnabled = !busy, menu = menu) {
                        picked = provider.provider
                        if (!provider.usable) openKey(provider.provider)
                    }
                }
            }
            // The cloud's own words for a create over the limit; a key removed from a row's `⋯` fails here, where it was asked.
            val note = state.cloudErrorAt(CloudErrorPlace.Create) ?: state.cloudKeyError()
                ?: "You can have up to ${offer.limit} cloud computers. Delete one to make another.".takeIf { full }
            note?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
        }
    }
}

/** A provider on its logo tile, with its name and its key: picked (checkmark) while its key is usable, else a tap picks it
 * for once it is and opens its key ([tap]). One with a usable key has a `⋯` ([menu]): its key, and removing it while no
 * cloud computer runs there; a spinner while it's being removed ([removing]). */
@Composable private fun ProviderRow(provider: CloudProvider, selected: Boolean, removing: Boolean, menuEnabled: Boolean,
    menu: () -> List<MenuItem>, tap: () -> Unit) {
    val credential = provider.credential
    val problem = credential?.problem
    val detail = when {
        problem != null -> tr(problem)
        credential != null -> "${provider.region} · ${credential.hint}"
        else -> "${provider.region} · ${tr(if (provider.offer != null) "Free sandbox available" else "Add a key first")}"
    }
    val action = if (provider.usable) Modifier.choice(selected, provider.name, choose = tap)
        else Modifier.plainButton(action = tap).semantics { contentDescription = provider.name }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.weight(1f).then(action), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.size(40.dp).clip(RoundedRectangle(11.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
                ProviderIcon(provider.provider, 22.dp)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(provider.name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(detail, style = Type.subheadline, color = if (problem != null) Palette.danger else Palette.secondary)
            }
            if (!provider.usable) Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
            else Symbol("checkmark", size = 15.dp, tint = Palette.text, modifier = Modifier.graphicsLayer { alpha = if (selected) 1f else 0f })
        }
        if (provider.usable) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
            if (removing) Spinner()
            else DropdownMenu(menu, Modifier.size(36.dp).testTag("cloudProviderKey.${provider.provider}"), enabled = menuEnabled, label = tr("More")) {
                Symbol("ellipsis", size = 18.dp, tint = Palette.secondary)
            }
        }
    }
}
