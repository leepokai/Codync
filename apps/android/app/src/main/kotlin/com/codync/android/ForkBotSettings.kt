package com.codync.android

import androidx.compose.runtime.Composable
import com.codync.android.core.BotDraft
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

/** Fork controls shared by the full editor and the details sheet's Settings page. */
@Composable internal fun ForkBotSettings(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    CardSection("Coordination") {
        row { SwitchRow("Chief of Staff (Coordinator)",
            "Supervises stalled or failing bots in teams and groups, with automatic triage and up to 2 retries per incident.",
            draft.isChief, { change(draft.copy(isChief = it)) }) }
    }
    ProactivePolicySection(draft, change)
    draft.id?.let { id ->
        if (draft.backend == "hermes" && state.bots.any { it.id == id && it.backend == "hermes" }) {
            HermesRuntimeCard(id, state, store)
        }
        DocumentsCard(id, state, store)
        ComputerPanelCard(id, state, store) { enabled, permissions ->
            change(draft.copy(computer = enabled, computerPermissions = permissions))
        }
    }
}

internal fun CardRows.botAccountRow(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    val info = backends(state).firstOrNull { it.text("id") == draft.backend }
    val supported = info?.get("accounts")?.jsonPrimitive?.booleanOrNull != false
    val canSignOut = info?.get("signOut")?.jsonPrimitive?.booleanOrNull == true
    if (!supported) return
    row {
        AgentAccountPicker(draft.backend, info?.text("name") ?: draft.backend, draft.account, state.actualConnection is LinkState.Ready,
            "${state.generation}/${state.computer?.id}/${draft.id}",
            supported, canSignOut,
            { store.agentAccounts(draft.backend) }, { store.authenticateAgentAccount(draft.backend) },
            { store.deleteAgentAccount(it) }, { store.signOutAgent(draft.backend) }) {
            change(draft.copy(account = it, model = null, modelOptions = emptyMap()))
        }
    }
}
