package com.codync.android.core

import kotlinx.serialization.Serializable

/** One of an agent's own `/` commands, as the agent lists them (ACP `available_commands_update`); [hint] is what to type after the name. */
@Serializable
data class AgentCommand(val name: String = "", val description: String = "", val hint: String? = null)
