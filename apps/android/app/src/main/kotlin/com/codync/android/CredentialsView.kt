package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.unit.dp
import com.codync.android.core.CredentialStatus
import com.codync.android.core.InstalledConnector
import com.codync.android.core.SavedLogin
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** Where the computer keeps secrets (iOS `CredentialsView`): connection keys, sign-ins and 1Password. */
@Composable internal fun CredentialsView(state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    var selected by remember { mutableStateOf<InstalledConnector?>(null) }
    var token by remember { mutableStateOf("") }
    var status by remember { mutableStateOf<CredentialStatus?>(null) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        store.refreshPlugins()
        try { status = store.credentialStatus() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't reach the credential store." }
    }
    fun save(value: String) {
        busy = true; error = null
        scope.launch {
            try { status = store.onePasswordToken(value); token = "" }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "1Password wasn't confirmed." }
            finally { busy = false }
        }
    }
    val saved = state.plugins?.connectors.orEmpty().filter { it.keys.isNotEmpty() }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Credentials")
        CardForm(Modifier.weight(1f)) {
            CardSection("Storage") {
                row { Text(tr(status?.provider ?: "System credential store"), style = Type.body, color = Palette.text) }
                row { Text(tr("Encrypted on the connected computer."), style = Type.body, color = Palette.secondary) }
            }
            CardSection("Saved connections") {
                for (connector in saved) row {
                    Row(Modifier.fillMaxWidth().plainButton { selected = connector }, verticalAlignment = Alignment.CenterVertically) {
                        Text(connector.name, Modifier.weight(1f), style = Type.body, color = Palette.text)
                        Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
                    }
                }
                if (saved.isEmpty()) row { Text(tr("No saved connection credentials yet."), style = Type.caption, color = Palette.secondary) }
                row { Text(tr("Values stay hidden. Select a connection to replace its credentials."), style = Type.caption, color = Palette.secondary) }
            }
            LoginsSection(store)
            CardSection("1Password", "Use a service account with read access only to a dedicated shared vault. The 1Password CLI must be installed on the computer.") {
                row { Text(tr(if (status?.onePasswordConnected == true) "Connected" else "Connect a shared vault"), style = Type.body, color = Palette.text) }
                row { FieldRow("Service account token") { PlainInput(token, { token = it }, "Paste your token", secret = true) } }
                row {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        PrimaryButton("Connect", enabled = !busy && token.isNotEmpty()) { save(token) }
                        if (status?.onePasswordConnected == true) SecondaryButton("Disconnect", enabled = !busy) { save("") }
                    }
                }
                row {
                    Text(tr("Use op://vault/item/field in a connector's credential field. Codync retrieves the value when the connector runs."),
                        style = Type.footnote, color = Palette.secondary)
                }
            }
            if (busy) Spinner()
            error?.let { ErrorRow(it) }
        }
    }
    CodyncSheet(selected, { selected = null }) { ConnectorCredentialEditor(it, store) }
}

/** Website and app logins bots type through the computer tool; passwords stay hidden. */
@Composable private fun LoginsSection(store: AppStore) {
    val scope = rememberCoroutineScope()
    var logins by remember { mutableStateOf(emptyList<SavedLogin>()) }
    var site by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    suspend fun load() {
        try { logins = store.savedLogins() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't load sign-ins." }
    }
    LaunchedEffect(Unit) { load() }
    fun run(work: suspend () -> Unit) {
        busy = true; error = null
        scope.launch {
            try { work() }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "That didn't go through." }
            finally { busy = false }
        }
    }
    CardSection("Sign-ins", "Bots type these into the matching website or app and never see the password. You can use an op:// reference.") {
        for (login in logins) row {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(login.site, style = Type.body, color = Palette.text)
                    if (login.username.isNotEmpty()) Text(login.username, style = Type.caption, color = Palette.secondary)
                }
                IconButton("Remove", "trash", enabled = !busy) { run { store.removeLogin(login.id); logins = logins - login } }
            }
        }
        row { FieldRow("Website or app") { PlainInput(site, { site = it }, "github.com") } }
        row { FieldRow("Username or email") { PlainInput(username, { username = it }, "name@example.com") } }
        row { FieldRow("Password") { PlainInput(password, { password = it }, "Password or op:// reference", secret = true) } }
        row {
            PrimaryButton("Save sign-in", enabled = !busy && site.isNotEmpty() && password.isNotEmpty()) {
                run { store.saveLogin(site, username, password); site = ""; username = ""; password = ""; load() }
            }
        }
        error?.let { row { Text(it, style = Type.footnote, color = Palette.danger) } }
    }
}

/** Replaces a connection's saved values; blank fields keep what's saved (iOS `ConnectorCredentialEditor`). */
@Composable private fun ConnectorCredentialEditor(connector: InstalledConnector, store: AppStore) {
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    var fields by remember(connector.id) { mutableStateOf(emptyMap<String, String>()) }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(connector.name) {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save credentials", "checkmark", enabled = fields.values.any(String::isNotEmpty)) {
                saving = true; error = null
                scope.launch {
                    try { store.updateConnectorCredentials(connector, fields.filterValues(String::isNotEmpty)); fields = emptyMap(); dismiss() }
                    catch (cancelled: CancellationException) { throw cancelled }
                    catch (failure: Exception) { error = failure.message ?: "The updated credentials weren't verified." }
                    finally { saving = false }
                }
            }
        }
        CardForm(Modifier.weight(1f)) {
            CardSection(footer = "Enter only the values you want to replace. You can use an op:// reference.") {
                for (key in connector.keys) row { PlainInput(fields[key].orEmpty(), { fields = fields + (key to it) }, key, secret = true) }
            }
            error?.let { ErrorRow(it) }
        }
    }
}
