package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.fill
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** iMate's personal token is one value on the computer, not a named login profile. */
@Composable internal fun ImateTokenForm(state: AppState, store: AppStore, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var token by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun save() {
        val value = token.trim()
        if (value.isEmpty() || busy) return
        busy = true
        error = null
        scope.launch {
            try {
                store.imateSetToken(value)
                back()
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                error = failure.message ?: "Couldn't save this token."
            } finally {
                busy = false
            }
        }
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ScreenHeader({ BackButton(back) }, {
            Text(tr("iMate token"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }) {
            if (busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save", "checkmark", enabled = token.isNotBlank(), action = ::save)
        }
        CardForm(Modifier.weight(1f)) {
            CardSection {
                row { NotedInput(token, { token = it }, tr("Personal token"), null, secret = true) }
                row {
                    Text(fill(tr("Create a personal token on the {name} open platform."), "name" to "iMate"),
                        style = Type.footnote, color = Palette.secondary)
                }
                row {
                    Text(fill(tr("Values are kept on {name} only and never shown again."), "name" to state.hostName),
                        style = Type.footnote, color = Palette.secondary)
                }
            }
            error?.let { CardSection { row { SelectionContainer { ErrorRow(it) } } } }
        }
    }
}
