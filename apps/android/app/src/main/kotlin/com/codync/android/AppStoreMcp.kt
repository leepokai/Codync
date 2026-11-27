package com.codync.android

import com.codync.android.core.McpApp
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

// The apps given MCP access on this computer: phones list them and revoke one (iOS `BotStore+McpAccess`).
// A failure shows as the store's error and keeps what is shown.

/** One read or revoke at a time, so a slower read never shows a list from before a revoke. */
// ponytail: one lock for every computer's list; one per store if several computers' sheets are open at once.
private val mcpCalls = Mutex()

/** Reads the list again (the sheet opening); a read already on its way stands for it. */
internal fun AppStore.loadMcpApps() = action("mcpApps") { mcpCalls.withLock { receiveMcpApps() } }

/** Its token stops working right away; the list is read again. Each app's revoke is its own action, so one never
 * waits out or drops another, nor a read. */
internal fun AppStore.revokeMcpApp(id: String) = action(revokeKey(id)) {
    mcpCalls.withLock {
        hostQuery("mcpClientRevoke", body("id" to id))
        receiveMcpApps()
    }
}

/** A revoke of app `id` is on its way. */
internal fun AppState.revokingMcpApp(id: String) = revokeKey(id) in busy

private fun revokeKey(id: String) = "mcpRevoke:$id"

private suspend fun AppStore.receiveMcpApps() {
    val apps = McpApp.active(hostQuery("mcpClients", body()))
    mutableState.update { it.copy(mcpApps = apps) }
}
