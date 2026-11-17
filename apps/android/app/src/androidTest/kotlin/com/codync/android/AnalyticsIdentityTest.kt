package com.codync.android

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class AnalyticsIdentityTest {
    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content

    /** Turning analytics off drops the queue, so nothing from this test reaches PostHog. */
    private fun dropQueue() {
        Analytics.setEnabled(false)
        Analytics.setEnabled(true)
    }

    // The iPhone's analyticsIsOnByDefaultAndIdentifiesOnlyTheFirstAccount, on real SharedPreferences.
    @Test fun theInstallIdMergesIntoTheFirstAccountOnly() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val stored = context.getSharedPreferences("codync-analytics", Context.MODE_PRIVATE)
        var events = emptyList<JsonObject>()
        instrumentation.runOnMainSync {
            Analytics.start(context)
            // The installed app's own settings, put back below.
            val sharing = Analytics.enabled
            val identified = stored.getStringSet("identified", null)
            val userId = Analytics.userId
            try {
                dropQueue()
                stored.edit().remove("identified").commit()
                for (user in listOf("user_1", "user_2")) {
                    Analytics.userId = { user }
                    Analytics.record("signed_in", emptyMap())
                }
                events = Analytics.pending.toList()
            } finally {
                dropQueue()
                Analytics.setEnabled(sharing)
                Analytics.userId = userId
                stored.edit().putStringSet("identified", identified).commit()
            }
        }
        assertEquals(
            listOf("\$identify" to "user_1", "signed_in" to "user_1", "signed_in" to "user_2"),
            events.map { it.text("event") to it.text("distinct_id") },
        )
        assertEquals(stored.getString("install", null), events[0].getValue("properties").jsonObject.text("\$anon_distinct_id"))
    }
}
