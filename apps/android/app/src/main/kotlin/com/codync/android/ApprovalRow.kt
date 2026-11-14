package com.codync.android

import android.util.Log
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import com.codync.android.core.AgentModels
import com.codync.android.core.ApprovalChoice
import com.codync.android.core.BotDraft
import com.codync.android.core.LinkState
import com.codync.android.core.withApproval
import com.codync.android.design.ChoicePicker
import com.codync.android.design.ValueRow
import com.codync.android.design.tr
import kotlinx.coroutines.CancellationException

/** Permissions: the modes the bot's agent offers (its model list), its Auto mode first, then approving everything. */
@Composable internal fun ApprovalRow(state: AppState, load: suspend (String) -> AgentModels, draft: BotDraft, change: (BotDraft) -> Unit,
    reload: Int = 0) {
    val online = state.actualConnection is LinkState.Ready
    var catalog by remember(draft.backend) { mutableStateOf<AgentModels?>(null) }
    // The host shares one model list asked for from several places at once (the model picker, here).
    // [reload]: the model picker's Retry, so a list that failed the first time fills both.
    LaunchedEffect(state.computer.id, draft.backend, online, reload) {
        if (draft.backend == "custom" || !online) return@LaunchedEffect
        try { catalog = load(draft.backend) }
        catch (cancelled: CancellationException) { throw cancelled }
        // The model picker says why; Auto, Ask me and Approve automatically stay on offer meanwhile.
        catch (error: Exception) { Log.i("Codync", "no agent modes for ${draft.backend}: ${error.message}") }
    }
    val (choices, selected) = ApprovalChoice.choices(draft, catalog)
    ValueRow("Permissions", detail = listOfNotNull(selected.note, tr(selected.detail)).joinToString(" ")) {
        ChoicePicker(selected.id, choices.map { it.id to it.label }, { change(draft.withApproval(it)) }, label = "Permissions")
    }
}
