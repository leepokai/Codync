package com.codync.android

import com.codync.android.core.SentinelReport
import com.codync.android.core.WireJson
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put

/** One thing `doctor` found wrong with a bot's setup. */
internal data class DoctorFinding(val botName: String, val isError: Boolean, val message: String)

internal data class DoctorReport(val checked: Int, val errors: Int, val findings: List<DoctorFinding>)

/** A daily token limit and today's use; `botId` is [FLEET_BUDGET] for all bots together. */
internal data class TokenBudget(val botId: String, val limit: Long, val used: Long, val level: String)

internal const val FLEET_BUDGET = "*"

/** Sentinel scans recent runtime events on the connected computer. */
internal suspend fun AppStore.sentinelReport(): SentinelReport =
    WireJson.decodeFromJsonElement(hostCall("sentinelHealth", buildJsonObject {}))

/** The host's upkeep calls the diagnostics sheet makes: bot check, daily token limits, failed runs, bot bundles. */
internal suspend fun AppStore.doctor(): DoctorReport {
    val answer = hostCall("doctor", buildJsonObject {}).jsonObject
    val findings = answer["findings"]?.jsonArray.orEmpty().map {
        val f = it.jsonObject
        DoctorFinding(f.text("botName").orEmpty(), f.text("severity") == "error", f.text("message").orEmpty())
    }
    return DoctorReport(answer["checked"]?.jsonPrimitive?.int ?: 0, answer["errors"]?.jsonPrimitive?.int ?: 0, findings)
}

internal suspend fun AppStore.tokenBudgets(): List<TokenBudget> = budgetsOf(hostCall("tokenBudgets", buildJsonObject {}))

/** A null [limit] takes the limit away. */
internal suspend fun AppStore.setTokenBudget(botId: String, limit: Long?): List<TokenBudget> =
    budgetsOf(hostCall("setTokenBudget", buildJsonObject {
        put("botId", botId)
        if (limit != null) put("limit", limit) else put("limit", JsonNull)
    }))

private fun budgetsOf(answer: kotlinx.serialization.json.JsonElement): List<TokenBudget> =
    (answer.jsonObject["budgets"] as? JsonArray).orEmpty().map {
        val b = it.jsonObject
        TokenBudget(b.text("botId").orEmpty(), b["limit"]?.jsonPrimitive?.long ?: 0, b["used"]?.jsonPrimitive?.long ?: 0, b.text("level").orEmpty())
    }

/** Clears the failed routine runs nobody is waiting on; answers how many. */
internal suspend fun AppStore.clearFailedRuns(): Int =
    hostCall("clearFailedRuns", buildJsonObject {}).jsonObject["cleared"]?.jsonPrimitive?.int ?: 0

/** Turns off, for every bot, the skills bots wrote that nobody has used for 30 days; answers how many switches. */
internal suspend fun AppStore.retireDormantSkills(): Int =
    hostCall("retireDormantSkills", buildJsonObject {}).jsonObject["turnedOff"]?.jsonPrimitive?.int ?: 0

/** The bot as bundle text, ready to copy. */
internal suspend fun AppStore.exportBot(botId: String): String = hostCall("exportBot", body("botId" to botId)).toString()

/** Makes the bots of a pasted bundle in [cwd]; answers how many. */
internal suspend fun AppStore.importBots(bundle: String, cwd: String): Int {
    val parsed = try { kotlinx.serialization.json.Json.parseToJsonElement(bundle) } catch (_: Exception) { error("That text isn't a bot bundle.") }
    val answer = hostCall("importBots", buildJsonObject { put("bundle", parsed); put("cwd", cwd) })
    return (answer.jsonObject["bots"] as? JsonArray)?.size ?: 0
}

/** The security decisions made on the computer, newest first. */
internal suspend fun AppStore.auditLog(): List<Pair<Long, Pair<String, String>>> = hostCall("auditLog", buildJsonObject {}).jsonObject["lines"]!!.jsonArray.map {
    val line = it.jsonObject
    line["at"]!!.jsonPrimitive.long to (line["action"]!!.jsonPrimitive.content to (line["detail"]?.jsonPrimitive?.content ?: ""))
}

/** Clears the computer's audit log. */
internal suspend fun AppStore.clearAuditLog() { hostCall("clearAuditLog", buildJsonObject {}) }
