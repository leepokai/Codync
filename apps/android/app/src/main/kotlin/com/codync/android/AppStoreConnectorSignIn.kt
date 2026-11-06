package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import android.net.Uri
import java.io.IOException
import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Connector sign-in in the browser, and its return to the app.

/** [declined] completes when the user cancels a sign-in with a code before its page opens. */
fun AppStore.connectorSignIn(id: String, requestId: String? = null, declined: CompletableDeferred<Unit>? = null) = action("oauth") {
    val generation = state.value.generation
    oauthRequestId = requestId
    // `showsCode`: this app can show a sign-in's code (GitHub); hosts send code sign-ins only to apps that can.
    val response = hostQuery("connectorSignIn", buildJsonObject { put("id", id); put("showsCode", true) }, 60_000)
    val plan = WireJson.decodeFromJsonElement<ConnectorSignIn>(response)
    val computer = requireNotNull(state.value.computer)
    externalHttps(plan.url)
    plan.code?.let { code ->
        if (!askSignInCode(code, declined)) { oauthRequestId = null; return@action }
        application.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Codync", code))
    }
    if (plan.callback == "app") {
        val pending = PendingConnectorOAuth.create(plan, id, computer.id, System.currentTimeMillis()).copy(requestId = requestId)
        withContext(Dispatchers.IO) { ConnectorOAuthBook(local.directory).save(pending) }
    } else {
        require(plan.callback == "host") { "This sign-in callback needs a newer app." }
        withContext(Dispatchers.IO) { ConnectorOAuthBook(local.directory).clear() }
    }
    val intent = android.content.Intent(android.content.Intent.ACTION_VIEW, Uri.parse(plan.url)).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
    try { application.startActivity(intent) }
    catch (error: Exception) { withContext(Dispatchers.IO) { ConnectorOAuthBook(local.directory).clear() }; throw IOException("Couldn't open the sign-in browser.", error) }
    if (plan.callback == "host") {
        val complete = withTimeoutOrNull(600_000) {
            while (true) {
                delay(2_000)
                combine(foregroundState, state) { visible, current -> visible && current.generation == generation && current.actualConnection is LinkState.Ready }.first { it }
                val listing = hostQuery("connectors", JsonObject(emptyMap()), 60_000, generation)
                val connectors = WireJson.decodeFromJsonElement<List<InstalledConnector>>(listing.jsonObject.getValue("items"))
                if (connectors.any { it.id == id && it.auth == "signedIn" }) {
                    requestId?.let { finishConnectionRequest(it, connectorId = id, expectedGeneration = generation) }
                    refreshPlugins()
                    return@withTimeoutOrNull true
                }
            }
            @Suppress("UNREACHABLE_CODE") false
        }
        if (complete == null) throw IOException("Sign-in wasn't confirmed. Check the computer and installed connections before trying again.")
    }
}
/** Shows a sign-in's [code] until Copy and open (true) or Cancel; a Cancel completes [declined] before the dialog closes. */
private suspend fun AppStore.askSignInCode(code: String, declined: CompletableDeferred<Unit>?): Boolean {
    mutableState.update { it.copy(signInCode = code) }
    return try { signInCodeAnswer.ask().also { if (!it) declined?.complete(Unit) } }
    finally { mutableState.update { it.copy(signInCode = null) } }
}
/** Copy and open on the sign-in code's dialog. */
fun AppStore.openSignInCode() = signInCodeAnswer.open()
/** The sign-in code's dialog closed: Cancel, answered a main-thread turn later. */
fun AppStore.dismissSignInCode() = signInCodeAnswer.dismissed { contextScope.launch(Dispatchers.Main) { it() } }
fun AppStore.cancelConnectorSignIn() = action("cancelOAuth") { cancelOAuthPending(null) }
fun AppStore.cancelConnectionRequestSignIn(entryId: String) = action("cancelOAuth") { cancelOAuthPending(entryId) }
internal suspend fun AppStore.cancelOAuthPending(requestId: String?) {
    val pending = withContext(Dispatchers.IO) { ConnectorOAuthBook(local.directory).read() }
    if (requestId != null && pending?.requestId != requestId && oauthRequestId != requestId) return
    val old = actions["oauth"]
    old?.cancelAndJoin()
    withContext(Dispatchers.IO) { ConnectorOAuthBook(local.directory).clear() }
    oauthRequestId = null
    refreshPlugins()
}
/** The browser came back with a connector sign-in: it finishes on the computer that started it. */
fun AccountStore.finishConnectorSignIn(uri: Uri) {
    linkJob?.cancel()
    linkJob = viewModelScope.launch {
        var expectedGeneration: Int? = null
        try {
            val back = OAuthReturn.parse(uri.toString(), BuildConfig.URL_SCHEME)
            val activated = deadline(35_000, "This account didn't finish opening.") { activated() }
            expectedGeneration = activated.generation
            val directory = local.directory
            val pending = withContext(Dispatchers.IO) { ConnectorOAuthBook(directory).read() }
                ?: throw IOException("No connector sign-in is pending for this account. Start sign-in again.")
            pending.validate(back, activated.computer?.id.orEmpty(), System.currentTimeMillis())
            val response = hostQuery("connectorSignInFinish", body("state" to back.state, "code" to back.code, "error" to back.error), 60_000, generation)
            val connector = WireJson.decodeFromJsonElement<InstalledConnector>(response.jsonObject.getValue("connector"))
            pending.requestId?.let { finishConnectionRequest(it, connectorId = connector.id, expectedGeneration = generation) }
            withContext(Dispatchers.IO) { ConnectorOAuthBook(directory).clear() }
            if (state.value.generation == generation) { oauthRequestId = null; refreshPlugins(); mutableState.update { it.copy(error = null) } }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) {
            mutableState.update {
                if (expectedGeneration != null && it.generation != expectedGeneration) it
                else it.copy(error = "Connector sign-in didn't complete. Reconnect and start sign-in again; the connector remains installed.")
            }
        }
    }
}

private suspend fun AppStore.finishConnectorSignIn(back: OAuthReturn, pending: PendingConnectorOAuth, directory: java.io.File) {
    val generation = state.value.generation
    val response = hostQuery("connectorSignInFinish", body("state" to back.state, "code" to back.code, "error" to back.error), 60_000, generation)
    val connector = WireJson.decodeFromJsonElement<InstalledConnector>(response.jsonObject.getValue("connector"))
    pending.requestId?.let { finishConnectionRequest(it, connectorId = connector.id, expectedGeneration = generation) }
    withContext(Dispatchers.IO) { ConnectorOAuthBook(directory).clear() }
    oauthRequestId = null; refreshPlugins(); mutableState.update { it.copy(error = null) }
}
