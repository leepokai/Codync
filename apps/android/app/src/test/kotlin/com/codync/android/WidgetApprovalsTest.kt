package com.codync.android

import com.codync.android.core.ApprovalPlan
import com.codync.android.core.Entry
import com.codync.android.core.NotificationActionPlan
import com.codync.android.core.WidgetActions
import com.codync.android.core.WireJson
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

class WidgetApprovalsTest {
    private fun permission(id: String, kind: String?, status: String = "pending", seq: Long = 1) = Entry(id, seq, "bot-$id", seq, "permission", data = buildJsonObject {
        put("status", status)
        kind?.let { put("toolKind", it) }
        put("title", "rm -rf /secret/project")
        put("command", "rm -rf /secret/project")
        put("options", buildJsonArray {
            add(buildJsonObject { put("kind", "allow_once"); put("optionId", "allow") })
            add(buildJsonObject { put("kind", "reject_once"); put("optionId", "reject") })
        })
    })

    @Test fun `only pending requests are reported, with their options and risk and never their text`() {
        val entries = listOf(permission("a", "execute"), permission("b", "read", seq = 2), permission("c", "read", status = "answered"),
            Entry("m", 5, "bot-a", 5, "agent"))
        val approvals = widgetApprovals(entries)
        assertEquals(listOf("b", "a"), approvals.map { it.entryId })
        assertEquals(listOf("low", "high"), approvals.map { it.risk })
        assertEquals("allow", approvals.first().allowOptionId)
        assertEquals("reject", approvals.first().denyOptionId)
        val stored = WireJson.encodeToString(approvals)
        assertFalse("rm -rf" in stored || "secret" in stored || "command" in stored)
    }

    @Test fun `a request of an unknown or missing kind is high risk and cannot be allowed from the widget`() {
        for (kind in listOf(null, "weird", "execute", "edit", "delete", "move")) {
            val approval = widgetApprovals(listOf(permission("x", kind))).single().copy(at = 100)
            assertEquals("high", approval.risk)
            assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval, allow = true, now = 101))
            assertEquals(ApprovalPlan.Send("reject"), WidgetActions.approval(approval, allow = false, now = 101))
        }
    }

    @Test fun `the work name of an answer is one per request and always valid`() {
        val nonce = WidgetActionWork.approvalNonce("entry/with weird:chars_123")
        assertNotNull(NotificationActionPlan.approval(nonce, "local", "AAAAAAAAAAAAAAAAAAAAAA", "bot", "entry1", "allow"))
        assertEquals(nonce, WidgetActionWork.approvalNonce("entry/with weird:chars_123"))
        assertNotNull(NotificationActionPlan.approval(WidgetActionWork.approvalNonce("a"), "local", "AAAAAAAAAAAAAAAAAAAAAA", "bot", "a", "allow"))
        assertEquals(WidgetActionWork.approvalNonce("e1"), WidgetActionWork.approvalNonce("e1"))
        assertNotEquals(WidgetActionWork.approvalNonce("e1"), WidgetActionWork.approvalNonce("e2"))
    }
}
