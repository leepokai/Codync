package com.codync.android

import androidx.compose.runtime.Composable
import com.codync.android.core.ApprovalChoice
import com.codync.android.core.BotDraft
import com.codync.android.core.withApproval
import com.codync.android.design.ChoicePicker
import com.codync.android.design.ValueRow
import com.codync.android.design.tr

/**
 * Permissions: the modes the bot's agent offers (its model list), its Auto mode first, then approving everything.
 * [reload]: the model picker's Retry, so a list that failed the first time fills both. The model picker says why one
 * failed; Auto, Ask me and Approve automatically stay on offer meanwhile.
 */
@Composable internal fun ApprovalRow(state: AppState, load: ModelLoad, draft: BotDraft, change: (BotDraft) -> Unit, reload: Int = 0) {
    val models = rememberModelCatalog(state, load, draft.backend, draft.loginProfile, reload)
    val (choices, selected) = ApprovalChoice.choices(draft, models.catalog)
    ValueRow("Permissions", detail = listOfNotNull(selected.note, tr(selected.detail)).joinToString(" ")) {
        ChoicePicker(selected.id, choices.map { it.id to it.label }, { change(draft.withApproval(it)) }, label = "Permissions")
    }
}
