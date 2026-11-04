package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WidgetActions
import com.codync.android.core.WidgetApproval
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

/** The most requests a report carries; a widget shows a few and opens the app for the rest. */
private const val MAX_APPROVALS = 12

/**
 * The permission requests waiting in the mirror, as a widget may act on them: which options answer them and how risky they are.
 * Never the command, the path or the title: a widget's storage holds no chat text (`WidgetApproval`).
 */
internal fun widgetApprovals(entries: List<Entry>): List<WidgetApproval> = entries
    .filter { it.kind == "permission" && it.data.text("status") == "pending" }
    .sortedByDescending { it.seq }
    .take(MAX_APPROVALS)
    .map { entry ->
        val options = (entry.data["options"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
        fun option(prefer: String, prefix: String) = (options.firstOrNull { it.text("kind") == prefer }
            ?: options.firstOrNull { it.text("kind")?.startsWith(prefix) == true })?.text("optionId")?.takeIf(String::isNotBlank)
        WidgetApproval(entry.botId, entry.id, option("allow_once", "allow"), option("reject_once", "reject"),
            WidgetActions.riskOf(entry.data.text("toolKind")))
    }
