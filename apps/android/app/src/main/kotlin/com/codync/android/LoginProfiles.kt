package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentBackend
import com.codync.android.core.LoginProfile
import com.codync.android.core.LoginProfileKind
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

private const val ADDING = "add"
private const val ADDING_KEY = "addKey"
private const val OWN = "own"
private const val NAME_LIMIT = 60

/** The computer's own login, as the agent's setup knows it: its sign-in state and how to ask again. */
internal class OwnLogin(val status: String, val checking: Boolean, val check: () -> Unit)

/**
 * An agent's login profiles in one place: the default one (the login the computer already has, with its sign-in options
 * as [ownRows]) and the others, each an account (the agent's own sign-in) or an API key (variables, edited in [editKeys]'
 * form). Each can be named; others are added, signed in or given keys, or removed. Bots pick theirs in their own
 * settings. [signIn] opens the sign-in terminal for an account; [signInNow] is one whose sign-in was asked for (a chat's
 * sign-in card), started here ([signInStarted]) when its agent signs itself in.
 */
@Composable internal fun LoginProfiles(backend: AgentBackend, host: String, store: AppStore, own: OwnLogin, signIn: (String) -> Unit,
    editKeys: (LoginProfile) -> Unit, signInNow: String?, signInStarted: () -> Unit, ownRows: CardRows.() -> Unit) {
    val scope = rememberCoroutineScope()
    // The login profile being changed, [OWN], [ADDING] or [ADDING_KEY].
    var busy by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var naming by remember { mutableStateOf<Naming?>(null) }
    fun run(key: String, change: suspend () -> Unit) {
        busy = key
        scope.launch {
            try { change(); error = null }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't check this agent." }
            finally { busy = null }
        }
    }
    // Its agent signs an account in itself (a browser on the computer), or its CLI does in a terminal.
    val inBrowser = backend.signsInBrowser
    suspend fun start(profile: String) = if (inBrowser) store.signInLoginProfile(backend.id, profile) else signIn(profile)
    fun rename(profile: String?, label: String?) {
        naming = Naming("Rename login profile", label.orEmpty()) { run(profile ?: OWN) { store.renameLoginProfile(backend.id, profile, it) } }
    }
    LaunchedEffect(Unit) {
        if (signInNow == null || !inBrowser) return@LaunchedEffect
        signInStarted()
        if (backend.loginProfiles?.firstOrNull { it.id == signInNow }?.signedIn == false) run(signInNow) { start(signInNow) }
    }

    val mine = backend.defaultLoginProfile
    val defaultName = mine?.name ?: tr("Default login profile")
    val defaultDetail = listOfNotNull(tr("Default"), mine?.email?.takeIf { mine.label != null }, tr(own.status))
    val unnamed = tr("New login profile")
    val signedIn = tr("Signed in")
    val signedOut = tr("Not signed in")
    val finishing = tr("Finish signing in in the browser on $host…")
    val apiKey = tr("API key")
    val noKey = tr("No key yet")
    CardSection("Login profiles", footer = "A bot picks its login profile under Agent in its settings. Bots that pick none use the default one.") {
        row {
            LoginProfileLine(defaultName, defaultDetail, { rename(null, mine?.label) }) {
                CheckAgain(own.checking, own.check)
            }
        }
        ownRows()
        for (profile in backend.loginProfiles.orEmpty()) row {
            val detail = if (profile.kind == LoginProfileKind.ApiKey) listOf(apiKey, profile.vars.joinToString(", ").ifEmpty { noKey })
                else listOfNotNull(profile.email?.takeIf { profile.label != null },
                    if (busy == profile.id && !profile.signedIn && inBrowser) finishing else if (profile.signedIn) signedIn else signedOut)
            LoginProfileLine(profile.name ?: unnamed, detail, { rename(profile.id, profile.label) }) {
                if (profile.kind == LoginProfileKind.ApiKey) StepAction("Edit keys", enabled = busy == null) { editKeys(profile) }
                else if (!profile.signedIn) StepAction("Sign in", enabled = busy == null) { run(profile.id) { start(profile.id) } }
                if (busy == profile.id) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
                else IconButton("Remove login profile", "trash", enabled = busy == null) { run(profile.id) { store.removeLoginProfile(profile.id) } }
            }
        }
        // An account signs in with the agent's own CLI, so it needs that installed; an agent that keeps one sign-in of
        // its own takes only API keys, which any agent can have.
        if (backend.loginProfileSignIn != null && backend.installed) row {
            OptionRow("Add account", "Signs in another ${backend.name} account in a ${if (inBrowser) "browser" else "terminal"} on $host.", "plus",
                busy = busy == ADDING, enabled = busy == null) {
                run(ADDING) { start(store.addLoginProfile(backend.id, LoginProfileKind.Account).id) }
            }
        }
        row {
            OptionRow("Add API key", "Runs ${backend.name} on keys you type or import from your login shell.", "key",
                busy = busy == ADDING_KEY, enabled = busy == null) {
                run(ADDING_KEY) { editKeys(store.addLoginProfile(backend.id, LoginProfileKind.ApiKey)) }
            }
        }
        error?.let { row { ErrorRow(it) } }
    }
    NameDialog(naming, "Login profile name", { naming = null }, { it.take(NAME_LIMIT) }) { it.trim() }
}

/** One login profile: its name, what's known of it, the pencil that renames it and its other actions. */
@Composable private fun LoginProfileLine(name: String, detail: List<String>, rename: () -> Unit, actions: @Composable RowScope.() -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(name, style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            SelectionContainer { Text(detail.joinToString(" · "), style = Type.subheadline, color = Palette.secondary) }
        }
        IconButton("Rename login profile", "pencil", action = rename)
        actions()
    }
}
