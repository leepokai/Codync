package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
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

/** Where New cloud computer is: picking the provider, connecting its key, or naming the computer. */
private sealed interface CloudStep {
    data object Provider : CloudStep
    data class Key(val provider: String) : CloudStep
    data class Name(val provider: String) : CloudStep
}

/**
 * Computers → Codync Cloud → "+", or Start a cloud computer on the first-run pages (iOS `NewCloudComputerSheet`), one sheet
 * that walks its steps in place: the provider the computer runs on (each with what it costs, or its key), its key on the
 * provider's site when it has none that works ([CloudKeyForm]), then the computer's name (empty, the suggested one). A
 * row's `⋯` opens a saved key, or removes it ([openKey]). Create closes the sheet once the computer is made, after
 * [created]; a refusal shows there, and at the account's limit why it's off. It opens on the providers every time.
 */
@Composable internal fun NewCloudComputerSheet(offer: CloudOffer, state: AccountContext, store: AccountStore, created: () -> Unit = {},
    openKey: (String) -> Unit) {
    DisposableEffect(store) {
        store.cloudSheetShown(true)
        onDispose { store.cloudSheetShown(false) }
    }
    var step by remember { mutableStateOf<CloudStep>(CloudStep.Provider) }
    // Kept across the steps, as typed.
    var name by remember { mutableStateOf("") }
    // A failure seen on the step being left is over.
    val go = { next: CloudStep ->
        store.clearCloudError(CloudErrorPlace.Create)
        for (provider in offer.providers) store.clearCloudError(CloudErrorPlace.Key(provider.provider))
        step = next
    }
    // A later step goes back to the providers, unless a key saves or the computer is being made.
    val canGoBack = !state.cloudBusy(offer)
    val back = { go(CloudStep.Provider) }
    BackHandler(step != CloudStep.Provider && canGoBack, back)
    AnimatedContent(step, transitionSpec = {
        val forward = targetState != CloudStep.Provider
        (slideInHorizontally(Motion.layoutOffset) { if (forward) it else -it } + fadeIn(Motion.fade())) togetherWith
            (slideOutHorizontally(Motion.layoutOffset) { if (forward) -it else it } + fadeOut(Motion.fade()))
    }, label = "New cloud computer step") { shown ->
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            when (shown) {
                CloudStep.Provider -> ProviderStep(offer, state, store, openKey) {
                    go(if (it.usable) CloudStep.Name(it.provider) else CloudStep.Key(it.provider))
                }
                is CloudStep.Key -> offer.providers.firstOrNull { it.provider == shown.provider }?.let { provider ->
                    ModalHeader({ StepTitle(tr("Connect {provider}").replace("{provider}", provider.name), canGoBack, back) })
                    // Its key kept, the computer is named next.
                    CardForm { CloudKeyForm(provider, offer, state, store) { go(CloudStep.Name(provider.provider)) } }
                }
                is CloudStep.Name -> offer.providers.firstOrNull { it.provider == shown.provider }?.let { provider ->
                    NameStep(provider, name, { name = it.take(CLOUD_NAME_LIMIT) }, offer, state, store, canGoBack, back, created)
                }
            }
        }
    }
}

/** A cloud action runs: a computer being made, or a key being saved or removed. One at a time, as on iOS. */
private fun AccountContext.cloudBusy(offer: CloudOffer) =
    CLOUD_CREATE in busy || offer.providers.any { cloudKeyAction(it.provider) in busy }

/** A later step's title, after the way back to the providers ([canGoBack]: off while a key saves or the computer is made). */
@Composable private fun StepTitle(title: String, canGoBack: Boolean, back: () -> Unit) {
    // Its glyph near the sheet's edge, as the close button's is on the other side.
    Row(Modifier.offset(x = (-8).dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton("Back", "chevron.left", enabled = canGoBack, action = back)
        Text(title, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Every provider the cloud offers; a row's `⋯` opens its saved key or removes it. A key removed from there fails here,
 * where it was asked. */
@Composable private fun ProviderStep(offer: CloudOffer, state: AccountContext, store: AccountStore, openKey: (String) -> Unit,
    pick: (CloudProvider) -> Unit) {
    ModalHeader("New cloud computer")
    CardForm {
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
                ProviderRow(provider, cloudKeyAction(provider.provider) in state.busy, enabled = !state.cloudBusy(offer), menu = menu) { pick(provider) }
            }
        }
        (state.cloudKeyError() ?: offer.limitNote())?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
    }
}

/** A provider on its logo tile, with its name and what to fix while it turns the key down, its key, or what it costs, and
 * a chevron: [tap] goes on with it. One with a usable key has a `⋯` ([menu]): its key, and removing it while no cloud
 * computer runs there; a spinner while it's being removed ([removing]). Both wait while another action runs ([enabled]). */
@Composable private fun ProviderRow(provider: CloudProvider, removing: Boolean, enabled: Boolean,
    menu: () -> List<MenuItem>, tap: () -> Unit) {
    val (detail, problem) = provider.detail(translator())
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.weight(1f).plainButton(enabled, action = tap).semantics { contentDescription = provider.name },
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.size(40.dp).clip(RoundedRectangle(11.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
                ProviderIcon(provider.provider, 22.dp)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(provider.name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(detail, style = Type.subheadline, color = if (problem) Palette.danger else Palette.secondary)
            }
        }
        if (provider.usable) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
            if (removing) Spinner()
            else DropdownMenu(menu, Modifier.size(36.dp).testTag("cloudProviderKey.${provider.provider}"), enabled = enabled, label = tr("More")) {
                Symbol("ellipsis", size = 18.dp, tint = Palette.secondary)
            }
        }
        Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
    }
}

/** The computer's [name] (empty, the suggested one) and where it will run; Create in the header, [created] once it's made. */
@Composable private fun NameStep(provider: CloudProvider, name: String, rename: (String) -> Unit, offer: CloudOffer, state: AccountContext,
    store: AccountStore, canGoBack: Boolean, back: () -> Unit, created: () -> Unit) {
    val dismiss = LocalDismissModal.current
    val suggested = offer.suggestedName(tr("Cloud computer"), tr("Cloud computer {n}"))
    val limit = offer.limitNote()
    // Its key can be turned down or removed meanwhile.
    val ready = provider.usable && !state.cloudBusy(offer) && limit == null
    val create = { if (ready) store.createCloudComputer(provider.provider, name.trim().ifEmpty { suggested }) { created(); dismiss() } }
    ModalHeader({ StepTitle(tr("New cloud computer"), canGoBack, back) }) {
        if (CLOUD_CREATE in state.busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
        else IconButton("Create", "checkmark", enabled = ready) { create() }
    }
    CardForm {
        CardSection("Name", footer = runsOn(provider.name, provider.region)) {
            row {
                val label = tr("Name your cloud computer")
                PlainTextField(name, rename, Modifier.fillMaxWidth().semantics { contentDescription = label }, suggested,
                    singleLine = true, keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
                    actions = KeyboardActions(onDone = { create() }))
            }
        }
        // The cloud's own words for a create it refused.
        (state.cloudErrorAt(CloudErrorPlace.Create) ?: limit)?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
    }
}
