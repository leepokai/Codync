package com.codync.android

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.BotDraft
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

/** A bot's settings in a sheet, as iOS `BotEditorView`: the whole settings form plus Close and Save. New bots start in [NewBotView]. */
@Composable fun BotEditor(state: AppState, initial: BotDraft, store: AppStore) {
    var draft by rememberBotDraft(initial, state)
    val online = state.actualConnection is LinkState.Ready
    val saving = "editor" in state.busy
    val dismiss = LocalDismissModal.current
    Column(Modifier.fillMaxSize()) {
        ModalHeader("Settings") {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save", "checkmark", enabled = draft.isValid && online) {
                store.saveBot(draft) { dismiss() }
            }
        }
        BotSettingsForm(state, store, draft, { draft = it }, FormPart.All, Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp))
    }
}

/** A form's draft of a saved bot: a change made elsewhere (a bot that manages the team, another device) replaces each field not edited here. */
@Composable internal fun rememberBotDraft(initial: BotDraft, state: AppState): MutableState<BotDraft> {
    val draft = remember(initial.id) { mutableStateOf(initial) }
    var base by remember(initial.id) { mutableStateOf(initial) }
    val saved = state.bots.firstOrNull { it.id == initial.id }?.let { BotDraft.from(it) }
    LaunchedEffect(saved) { if (saved != null && saved != base) { draft.value = draft.value.rebased(base, saved); base = saved } }
    return draft
}

/** The computer's agents, as its hello lists them. */
internal fun backends(state: AppState): List<JsonObject> = (state.hello?.get("backends") as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
internal val JsonObject.available get() = this["available"]?.jsonPrimitive?.booleanOrNull == true
internal val JsonObject.curated get() = this["curated"]?.jsonPrimitive?.booleanOrNull != false
