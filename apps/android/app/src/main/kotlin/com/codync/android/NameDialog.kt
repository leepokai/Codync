package com.codync.android

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import com.codync.android.design.*

/** An open name dialog (desktop `Naming`): its title, the name it opens with, and what Save does with the typed one. */
internal class Naming(val title: String, val name: String, val save: (String) -> Unit)

/**
 * Asks for a name (a section's, an artifact's, a cloud computer's) while [naming] is set (desktop `NameDialog`): [prompt] in
 * the empty field, [limit] keeps it to what's kept, [clean] reads the typed text as the name to save (null: nothing to save).
 * Save passes on a name only when it changed. Each opening ([naming] is a new object every time) starts from its own name,
 * the cursor at its end; the last one stays on show while the dialog fades out.
 */
@Composable internal fun NameDialog(naming: Naming?, prompt: String, dismiss: () -> Unit, limit: (String) -> String = { it },
    clean: (String) -> String? = { it.trim().ifEmpty { null } }) {
    var opened by remember { mutableStateOf<Naming?>(null) }
    var name by remember { mutableStateOf(TextFieldValue()) }
    if (naming != null && naming !== opened) {
        opened = naming
        name = TextFieldValue(naming.name, TextRange(naming.name.length))
    }
    val submit = { if (naming != null) clean(name.text)?.takeIf { it != naming.name }?.let(naming.save) }
    val focus = remember { FocusRequester() }
    CodyncDialog(opened?.title.orEmpty(), naming != null, dismiss, field = {
        LaunchedEffect(Unit) { focus.requestFocus() }
        PlainTextField(name, { name = it.copy(text = limit(it.text)) }, Modifier.fillMaxWidth().pill().focusRequester(focus),
            prompt, singleLine = true, keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
            actions = KeyboardActions(onDone = { dismiss(); submit() }))
    }) { listOf(DialogAction("Save", action = submit)) }
}
