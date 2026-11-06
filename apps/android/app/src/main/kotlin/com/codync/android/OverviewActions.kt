package com.codync.android

import android.content.Context
import androidx.glance.GlanceId
import androidx.glance.action.ActionParameters
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.updateAll
import com.codync.android.core.ApprovalPlan
import com.codync.android.core.NotificationActionPlan
import com.codync.android.core.WidgetActions

internal val EntryKey = ActionParameters.Key<String>("entry")
internal val AllowKey = ActionParameters.Key<Boolean>("allow")

/**
 * Allow or Deny on a permission request, from the widget. The decision comes from `WidgetActions`, which refuses to allow anything that
 * changes the computer or that the report didn't mark low risk; it is checked again here because the report may have moved since the
 * card was drawn. What it can't do from here opens the app on that bot, where the fingerprint is asked as always.
 */
class ApprovalActionCallback : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val entry = parameters[EntryKey] ?: return
        val allow = parameters[AllowKey] ?: false
        val feed = WidgetStore.feed(context)
        val owner = WidgetStore.owner(context)
        val computer = feed.computerId
        val approval = feed.approvals.firstOrNull { it.entryId == entry }
        // One request is answered once: the first tap wins, and the card says "Sending…" until the computer answered.
        if (WidgetStore.isSending(context, entry)) return
        val plan = WidgetActions.approval(approval, allow)
        val action = (plan as? ApprovalPlan.Send)?.let { send ->
            if (owner == null || computer == null || approval == null) null
            else NotificationActionPlan.approval(WidgetActionWork.approvalNonce(entry), owner.scope, computer, approval.botId, entry, send.optionId)
        }
        if (action == null) {
            context.startActivity(widgetIntent(context, feed, approval?.botId))
            return
        }
        WidgetStore.sending(context, entry)
        WidgetActionWork.enqueue(context, action)
        OverviewWidget().updateAll(context)
    }
}
