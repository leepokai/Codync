package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentBackend
import com.codync.android.core.KeyRow
import com.codync.android.core.LoginProfile
import com.codync.android.core.keyRows
import com.codync.android.core.keysToSave
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * An API key login profile's variables, kept on the computer (iOS `LoginProfileKeysForm`): typed here, or copied from the
 * computer's login shell (importing saves what was typed too). Values go one way: the host sends back only names.
 */
@Composable internal fun LoginProfileKeysForm(backend: AgentBackend, profile: LoginProfile, state: AppState, store: AppStore, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val host = state.hostName
    var rows by remember { mutableStateOf(backend.keyRows(profile)) }
    // Saved variables taken off the form: Save sends each with an empty value.
    var removed by remember { mutableStateOf(emptyList<String>()) }
    // The login shell's variable names, once asked for, and the ones ticked to import.
    var shell by remember { mutableStateOf<List<String>?>(null) }
    var ticked by remember { mutableStateOf(emptySet<String>()) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun run(change: suspend () -> Unit) {
        busy = true; error = null
        scope.launch {
            try { change() }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't update these keys." }
            finally { busy = false }
        }
    }
    fun edit(index: Int, change: (KeyRow) -> KeyRow) { rows = rows.mapIndexed { at, row -> if (at == index) change(row) else row } }
    val toSave = rows.keysToSave(removed)
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ScreenHeader({ BackButton(back) }, { Text(profile.name ?: tr("API key"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }) {
            if (busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save", "checkmark", enabled = toSave.isNotEmpty()) { run { store.setLoginProfileKeys(profile.id, vars = toSave); back() } }
        }
        CardForm(Modifier.weight(1f)) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                CardSection {
                    for ((index, key) in rows.withIndex()) row {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                if (!key.named) PlainInput(key.name, { name -> edit(index) { it.copy(name = name) } }, "Variable name")
                                NotedInput(key.value, { value -> edit(index) { it.copy(value = value) } }, if (key.saved) "Saved (type to replace)" else "Value",
                                    key.name.takeIf { key.named }, secret = true)
                            }
                            IconButton("Remove", "trash", enabled = !busy) {
                                rows = rows.filterIndexed { at, _ -> at != index }
                                if (key.saved) removed = removed + key.name
                            }
                        }
                    }
                    row { LinkButton("Add variable", enabled = !busy) { rows = rows + KeyRow() } }
                    row { LinkButton("Import from login shell", enabled = !busy) { run { shell = store.loginShellVariables() } } }
                }
                Text(tr("Values are kept on $host only and never shown again."), Modifier.padding(horizontal = 4.dp), style = Type.caption, color = Palette.tertiary)
            }
            AnimatedVisibility(shell != null, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
                CardSection("Import from login shell", accessory = {
                    StepAction("Import", enabled = ticked.isNotEmpty() && !busy) { run { store.setLoginProfileKeys(profile.id, toSave, ticked.toList()); back() } }
                }) {
                    row { Text(tr("Pick the variables this login profile should use. Their values are copied now and kept on $host."), style = Type.footnote, color = Palette.secondary) }
                    for (name in shell.orEmpty()) row {
                        val on = name in ticked
                        Row(Modifier.fillMaxWidth().semantics { selected = on }.plainButton(role = Role.Checkbox) { ticked = if (on) ticked - name else ticked + name },
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text(name, Modifier.weight(1f), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            if (on) Symbol("checkmark", size = 13.dp, tint = Palette.text)
                        }
                    }
                }
            }
            error?.let { CardSection { row { ErrorRow(it) } } }
        }
    }
}
