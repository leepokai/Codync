package com.codync.android.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

/**
 * The demo's data: sample bots, a session with steps and a diff, an approval waiting for the person and a routine, all in
 * memory. While demo mode is on the app shows this instead of the computer's data and refuses every call to the computer;
 * leaving it brings back what was there, untouched. Pure data: no file, no network, no clock but the one it is given.
 */
object DemoScript {
    const val COMPUTER_ID = "demo-computer"
    const val SCOUT = "demo-scout"
    const val PACER = "demo-pacer"
    const val SCRIBE = "demo-scribe"

    val computer = Computer(id = COMPUTER_ID, name = "Demo computer", signKey = "")

    private fun bot(id: String, name: String, description: String, shape: String, color: String, status: String, activity: String, now: Long, unread: Int): Bot =
        WireJson.decodeFromJsonElement(Bot.serializer(), buildJsonObject {
            put("id", id); put("name", name); put("description", description); put("avatarShape", shape); put("avatarColor", color)
            put("backend", "claude"); put("permission", "ask"); put("status", status); put("activity", activity); put("unread", unread)
            put("lastAt", now); put("createdAt", now - 86_400_000); put("kind", "agent"); put("cwd", "/demo/pace")
        })

    fun bots(now: Long): List<Bot> = listOf(
        bot(SCOUT, "Scout", "Watches the tests", "squircle", "green", "idle", "", now - 60_000, 0),
        bot(PACER, "Pacer", "Owns the pace math", "blob", "orange", "needsInput", "Needs approval: Edit src/pace.js", now - 30_000, 1),
        bot(SCRIBE, "Scribe", "Writes the changelog", "circle", "blue", "idle", "", now - 3_600_000, 0),
    )

    private fun entry(seq: Long, bot: String, kind: String, turn: Long, at: Long, data: JsonObject) =
        Entry(id = "demo-$seq", seq = seq, botId = bot, rev = 10 + seq, kind = kind, turn = turn, data = data, createdAt = at, updatedAt = at)

    /** A session with a diff (Scout) and an approval waiting (Pacer). */
    fun entries(now: Long): List<Entry> = listOf(
        entry(1, SCOUT, "user", 1, now - 600_000, buildJsonObject { put("text", "Why is the weekly total wrong on Mondays?"); put("status", "sent") }),
        entry(2, SCOUT, "tool", 1, now - 590_000, buildJsonObject { put("title", "Read src/pace.js"); put("toolKind", "read"); put("status", "completed") }),
        entry(3, SCOUT, "tool", 1, now - 580_000, buildJsonObject {
            put("title", "Edit src/pace.js"); put("toolKind", "edit"); put("status", "completed")
            putJsonArray("diffs") {
                add(buildJsonObject {
                    put("path", "src/pace.js"); put("added", 3); put("removed", 0); put("isNew", false); put("startLine", 18)
                    put("patch", "@@ -18,0 +18,3 @@\n+function weeklyTotals(rows) {\n+  return rows.reduce((sum, r) => sum + r.km, 0)\n+}\n")
                })
            }
        }),
        entry(4, SCOUT, "agent", 1, now - 570_000, buildJsonObject {
            put("text", "The week started on Sunday for the total but Monday for the chart. I made both use the ISO week and added a test.")
            put("final", true)
        }),
        entry(5, PACER, "permission", 2, now - 30_000, buildJsonObject {
            put("title", "Edit src/pace.js"); put("toolKind", "edit"); put("status", "pending"); put("author", PACER); put("detail", "")
            put("diffs", buildJsonArray { }); put("selected", JsonPrimitive(null as String?))
            putJsonArray("options") {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
                add(buildJsonObject { put("optionId", "deny"); put("name", "Deny"); put("kind", "reject_once") })
            }
        }),
    )

    /** The routine of the bot that watches the tests. */
    fun routines(now: Long): Map<String, RoutineListing> {
        val routine = Routine(
            id = "demo-routine", botId = SCOUT, name = "Morning check", instruction = "Run the tests and tell me what broke overnight.",
            triggers = emptyList(), enabled = true, createdAt = now - 86_400_000, updatedAt = now - 3_600_000,
            triggerDescriptions = listOf("Every weekday at 9:00"), nextRunAt = now + 12 * 3_600_000, timeoutSeconds = 600,
        )
        return mapOf(SCOUT to RoutineListing(listOf(routine), emptyList()))
    }

    /** The words the app says when something would change the world outside the demo. */
    const val REFUSAL = "Demo mode: nothing is sent or changed. Leave it to use your bots."
}
