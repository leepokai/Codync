package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.ApprovalEvaluation
import com.codync.android.core.ApprovalRule
import com.codync.android.core.ApprovalRules
import com.codync.android.core.BotDraft
import com.codync.android.design.*
import kotlinx.coroutines.launch

/**
 * A bot's approval rules, first match first: add, edit, reorder, delete, and try a command against them. The host
 * decides what a rule does (`testApprovalRule`); nothing here copies its logic.
 */
internal fun CardRows.approvalRulesRows(state: AppState, store: AppStore, draft: BotDraft, savedProfile: String?, change: (BotDraft) -> Unit) {
    row { SecurityProfileChooser(state, store, draft, savedProfile, change) }
    val rules = draft.approvalRules.orEmpty()
    fun set(next: List<ApprovalRule>) = change(draft.copy(approvalRules = next))
    row {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(tr("Approval rules"), style = Type.body, color = Palette.text)
            Text(tr("The first rule that matches decides, before the Approvals setting.") + " " + tr(ApprovalRules.CHAINING_WARNING),
                style = Type.caption, color = Palette.secondary)
            if (rules.isEmpty()) Text(tr("No rules: the Approvals setting decides everything."), style = Type.caption, color = Palette.tertiary)
        }
    }
    for ((index, rule) in rules.withIndex()) row {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("${index + 1}", style = Type.footnote, color = Palette.tertiary)
                ChoicePicker(rule.decision, ApprovalRules.DECISIONS, { set(rules.mapIndexed { i, r -> if (i == index) r.copy(decision = it) else r }) },
                    label = "Rule decision")
                ChoicePicker(rule.kind.orEmpty(), listOf("" to "Any tool") + ApprovalRules.KINDS.map { it to it },
                    { set(rules.mapIndexed { i, r -> if (i == index) r.copy(kind = it.ifEmpty { null }) else r }) }, label = "Tool kind")
            }
            FieldBox(rule.pattern, { text -> set(rules.mapIndexed { i, r -> if (i == index) r.copy(pattern = text) else r }) },
                "e.g. git status*", 1, 3, capitalize = false)
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                IconButton("Move rule up", "chevron.up", enabled = index > 0) { set(ApprovalRules.move(rules, index, index - 1)) }
                IconButton("Move rule down", "chevron.down", enabled = index < rules.lastIndex) { set(ApprovalRules.move(rules, index, index + 1)) }
                IconButton("Delete rule", "trash") { set(rules.filterIndexed { i, _ -> i != index }) }
            }
        }
    }
    row { SecondaryButton(tr("Add rule")) { set(rules + ApprovalRule(pattern = "", decision = "ask")) } }
    quickRows(draft, change)
    row { RuleTester(store, draft, rules) }
}

/**
 * The quick rules by kind of action, under the person's own rules. Shown against a host that has them; off, nothing
 * below is enforced. Browser, apps, team and routine tools belong to the connectors' tool policies.
 */
private fun CardRows.quickRows(draft: BotDraft, change: (BotDraft) -> Unit) {
    val on = draft.actionRulesOn ?: return
    row {
        SwitchRow("Quick rules by kind of action", tr(if (on) "After your rules above, these decide by the kind of tool. " else "Off: nothing below is enforced. ") +
            tr("Browser, apps, team and routine tools are governed by the connectors' tool policies, not here."),
            on, { change(draft.copy(actionRulesOn = it)) })
    }
    for ((id, label, kinds) in ApprovalRules.QUICK_CLASSES) row {
        ValueRow(label, detail = kinds) {
            ChoicePicker(draft.actionRules[id].orEmpty(), ApprovalRules.QUICK_POLICIES,
                { change(draft.copy(actionRules = if (it.isEmpty()) draft.actionRules - id else draft.actionRules + (id to it))) }, label = label)
        }
    }
}

/** "Try a command": asks the host what the bot would do with it under the rules as they are in the form now. */
@Composable
private fun RuleTester(store: AppStore, draft: BotDraft, rules: List<ApprovalRule>) {
    var command by remember { mutableStateOf("") }
    var answer by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val cant = tr("This computer's host can't test rules.")
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        FieldBox(command, { command = it; answer = null }, "Try a command, e.g. git push origin main", 1, 2, capitalize = false)
        SecondaryButton(tr("Test"), enabled = command.isNotBlank() && draft.id != null) {
            scope.launch {
                val evaluation: ApprovalEvaluation? = store.testApprovalRule(draft.id.orEmpty(), command, ApprovalRules.clean(rules), draft.actionRulesOn?.let { it to draft.actionRules })
                answer = evaluation?.describe() ?: cant
            }
        }
        answer?.let { Text(it, style = Type.caption, color = Palette.secondary) }
    }
}
