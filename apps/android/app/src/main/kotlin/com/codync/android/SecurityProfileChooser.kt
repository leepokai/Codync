package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import com.codync.android.core.BotDraft
import com.codync.android.core.SecurityProfile
import com.codync.android.design.*

/**
 * The security profile of a bot, chosen when it is made or later: one sentence per profile and what choosing it sets. A
 * bot with its own rules shows "Custom rules". The host writes the rules; this lists the choices it offers.
 */
@Composable
internal fun SecurityProfileChooser(state: AppState, store: AppStore, draft: BotDraft, saved: String?, change: (BotDraft) -> Unit) {
    var profiles by remember(state.computers.firstOrNull()?.id) { mutableStateOf<List<SecurityProfile>?>(null) }
    LaunchedEffect(state.computers.firstOrNull()?.id) { profiles = store.securityProfiles() }
    // An older host has no profiles: nothing to choose.
    val list = profiles?.takeIf { it.isNotEmpty() } ?: return
    val chosen = list.firstOrNull { it.id == draft.securityProfile }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        ValueRow("Security profile", detail = "How much this bot may do without asking. You can still edit the rules below.") {
            ChoicePicker(draft.securityProfile.orEmpty(), listOf("" to "Custom rules") + list.map { it.id to it.title }, { id ->
                val profile = list.firstOrNull { it.id == id }
                // The form shows what the profile will write; the host writes it on save.
                change(if (profile == null) draft.copy(securityProfile = null)
                    else draft.copy(securityProfile = profile.id, permission = profile.permission, approvalRules = profile.approvalRules()))
            }, label = "Security profile")
        }
        if (chosen != null) {
            Text(tr(chosen.title), style = Type.subheadline.semibold, color = Palette.text)
            Text(tr(chosen.summary), style = Type.caption, color = Palette.secondary)
            for (line in chosen.preview()) Text(tr(line), style = Type.caption, color = Palette.tertiary)
            if (draft.securityProfile != saved) {
                Text(tr("Saving replaces this bot's Approvals setting and rules with the profile's."), style = Type.caption, color = Palette.warning)
            }
        }
    }
}
