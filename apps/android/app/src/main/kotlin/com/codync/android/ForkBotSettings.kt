package com.codync.android

import androidx.compose.runtime.Composable
import com.codync.android.core.BotDraft
import com.codync.android.core.LinkState
import com.codync.android.design.*

/** Fork controls shared by the full editor and the details sheet's Settings page. */
@Composable internal fun ForkBotSettings(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    CardSection("Coordination") {
        row { SwitchRow("Chief of Staff (Coordinator)",
            "Supervises stalled or failing bots in teams and groups, with automatic triage and up to 2 retries per incident.",
            draft.isChief, { change(draft.copy(isChief = it)) }) }
    }
    ProactivePolicySection(draft, change)
    draft.id?.let { id ->
        DocumentsCard(id, state, store)
        ComputerPanelCard(id, state, store) { enabled, permissions ->
            change(draft.copy(computer = enabled, computerPermissions = permissions))
        }
    }
}

internal fun CardRows.botAccountRow(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    if (draft.backend != "antigravity") return
    row {
        AgentAccountPicker(draft.backend, draft.account, state.actualConnection is LinkState.Ready,
            "${state.generation}/${state.computer?.id}/${draft.id}",
            { store.agentAccounts(draft.backend) }, { store.authenticateAgentAccount(draft.backend) }) {
            change(draft.copy(account = it, model = null, modelOptions = emptyMap()))
        }
    }
}
