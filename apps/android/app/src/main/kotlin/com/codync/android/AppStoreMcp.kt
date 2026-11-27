package com.codync.android

import com.codync.android.core.McpApp
import kotlinx.coroutines.flow.update

// The apps given MCP access on this computer: phones list them and revoke one (iOS `BotStore+McpAccess`).
// A failure shows as the store's error and keeps what is shown.

/** Reads the list again (the sheet opening). */
internal fun AppStore.loadMcpApps() = action("mcpApps") { receiveMcpApps() }

/** Its token stops working right away; the list is read again. */
internal fun AppStore.revokeMcpApp(id: String) = action("mcpApps") {
    hostQuery("mcpClientRevoke", body("id" to id))
    receiveMcpApps()
}

private suspend fun AppStore.receiveMcpApps() {
    val apps = McpApp.active(hostQuery("mcpClients", body()))
    mutableState.update { it.copy(mcpApps = apps) }
}
