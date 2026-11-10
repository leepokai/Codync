package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
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
import com.codync.android.design.*
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put


private data class EnterpriseMember(val id: String, val userName: String, val displayName: String)

private suspend fun AppStore.enterpriseMembers(): List<EnterpriseMember> {
    val list = hostQuery("members", buildJsonObject { }).jsonObject["members"]?.jsonArray ?: return emptyList()
    return list.mapNotNull { item ->
        val row = item.jsonObject
        val id = row["id"]?.jsonPrimitive?.content ?: return@mapNotNull null
        EnterpriseMember(id, row["userName"]?.jsonPrimitive?.content.orEmpty(), row["displayName"]?.jsonPrimitive?.content.orEmpty())
    }
}

private suspend fun AppStore.addEnterpriseMember(email: String, name: String) {
    hostQuery("addMember", buildJsonObject { put("email", email); put("name", name) })
}

private suspend fun AppStore.removeEnterpriseMember(id: String) {
    hostQuery("removeMember", buildJsonObject { put("id", id) })
}

private data class EnterpriseStatus(
    val configured: Boolean = false,
    val issuer: String = "",
    val clientId: String = "",
    val domains: List<String> = emptyList(),
    val scimTokenSet: Boolean = false,
)

private suspend fun AppStore.enterpriseStatus(): EnterpriseStatus {
    val j = hostQuery("enterpriseStatus", buildJsonObject {}).jsonObject
    return EnterpriseStatus(
        configured = j["configured"]?.jsonPrimitive?.content == "true",
        issuer = j["issuer"]?.jsonPrimitive?.content.orEmpty(),
        clientId = j["clientId"]?.jsonPrimitive?.content.orEmpty(),
        domains = j["domains"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty(),
        scimTokenSet = j["scimTokenSet"]?.jsonPrimitive?.content == "true",
    )
}

private suspend fun AppStore.setEnterprise(issuer: String, clientId: String, domains: List<String>) {
    hostQuery("setEnterprise", buildJsonObject {
        put("issuer", issuer); put("clientId", clientId)
        put("domains", kotlinx.serialization.json.JsonArray(domains.map { kotlinx.serialization.json.JsonPrimitive(it) }))
    })
}

/** A company's OpenID Connect sign-in and a SCIM token; the provider is checked before saving. */
@Composable internal fun EnterpriseSection(state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    var status by remember { mutableStateOf<EnterpriseStatus?>(null) }
    var issuer by remember { mutableStateOf("") }
    var clientId by remember { mutableStateOf("") }
    var domains by remember { mutableStateOf("") }
    var token by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var members by remember { mutableStateOf<List<EnterpriseMember>>(emptyList()) }
    var email by remember { mutableStateOf("") }
    var personName by remember { mutableStateOf("") }
    fun loadMembers() = scope.launch {
        members = runCatching { store.enterpriseMembers() }.getOrDefault(emptyList())
    }
    fun load() = scope.launch { try { status = store.enterpriseStatus() } catch (e: Exception) { error = e.message } }
    LaunchedEffect(state.contextId) { load() }
    LaunchedEffect(state.contextId, status?.configured) { if (status?.configured == true) loadMembers() }
    CardSection("Company sign-in", "An OpenID Connect provider such as Microsoft Entra ID, Okta or Google Workspace. Discovery is checked before saving. SCIM lists the owner and this computer's devices (read-only).") {
        row {
            val shown = status ?: return@row
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (!shown.configured) {
                    FieldBox(issuer, { issuer = it }, "Issuer (https://…)", 1, 2, fill = Palette.bubbleUser, capitalize = false)
                    FieldBox(clientId, { clientId = it }, "Client id", 1, 1, fill = Palette.bubbleUser, capitalize = false)
                    FieldBox(domains, { domains = it }, "Email domains, comma separated", 1, 2, fill = Palette.bubbleUser, capitalize = false)
                    SecondaryButton(if (busy) "Checking…" else "Connect provider", enabled = !busy && issuer.isNotBlank() && clientId.isNotBlank()) {
                        busy = true
                        scope.launch {
                            try { store.setEnterprise(issuer.trim(), clientId.trim(), domains.split(',', ' ').filter { it.isNotBlank() }); load() }
                            catch (e: Exception) { error = e.message } finally { busy = false }
                        }
                    }
                } else {
                    Text(shown.issuer, style = Type.caption, color = Palette.text)
                    Text("Domains: ${shown.domains.joinToString(", ")}", style = Type.caption, color = Palette.secondary)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        SecondaryButton("New SCIM token", enabled = !busy) {
                            scope.launch { try { token = store.hostQuery("rotateScim", buildJsonObject {}).jsonObject["token"]?.jsonPrimitive?.content } catch (e: Exception) { error = e.message } }
                        }
                        SecondaryButton("Disconnect", enabled = !busy) {
                            scope.launch { try { store.hostQuery("disconnectEnterprise", buildJsonObject {}); load() } catch (e: Exception) { error = e.message } }
                        }
                    }
                    token?.let { Text("SCIM token (shown once): $it", style = Type.caption, color = Palette.warning) }
                    Text("SCIM URL: /scim/v2/Users on the host's local port.", style = Type.caption, color = Palette.secondary)
                    Text("People: ${members.size}", style = Type.caption, color = Palette.text)
                    for (member in members) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(member.displayName.ifBlank { member.userName }, Modifier.weight(1f), style = Type.caption, color = Palette.text)
                            SecondaryButton("Remove", enabled = !busy) {
                                scope.launch { runCatching { store.removeEnterpriseMember(member.id) }; loadMembers() }
                            }
                        }
                    }
                    FieldBox(email, { email = it }, "Email", 1, 1, fill = Palette.bubbleUser, capitalize = false)
                    FieldBox(personName, { personName = it }, "Name", 1, 1, fill = Palette.bubbleUser)
                    SecondaryButton("Add", enabled = !busy && email.isNotBlank()) {
                        scope.launch {
                            try { store.addEnterpriseMember(email.trim(), personName.trim()); email = ""; personName = ""; loadMembers() }
                            catch (e: Exception) { error = e.message }
                        }
                    }
                }
                error?.let { Text(it, style = Type.caption, color = Palette.danger) }
            }
        }
    }
}
