package com.codync.android

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import com.codync.android.core.EncryptedChannel
import com.codync.android.core.HostConnector
import com.codync.android.core.HostError
import com.codync.android.core.NotificationAction
import com.codync.android.core.NotificationActionPlan
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * What a widget sends to the computer: an answer to a permission request or a quick message. Durable like the notification actions
 * (a dead process or a lost connection doesn't lose it, one nonce is one action so a repeated tap sends nothing twice), but tied to
 * the account the widgets follow, not to push. Nothing is called done until the computer answered.
 */
internal object WidgetActionWork {
    private const val KIND = "kind"; private const val NONCE = "nonce"; private const val SCOPE = "scope"; private const val COMPUTER = "computer"
    private const val BOT = "bot"; private const val ENTRY = "entry"; private const val OPTION = "option"; private const val MESSAGE = "message"

    /** One nonce per request: the first answer given to it wins, whatever else is tapped afterwards. */
    fun approvalNonce(entryId: String): String = "widget-" + entryId.map { if (it.isLetterOrDigit() && it.code < 128 || it == '-') it else '-' }.joinToString("").take(50).padEnd(8, '0')

    fun enqueue(context: Context, action: NotificationAction) {
        val request = OneTimeWorkRequestBuilder<WidgetActionWorker>().setInputData(data(action))
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 15, TimeUnit.SECONDS).build()
        WorkManager.getInstance(context).enqueueUniqueWork(NotificationActionPlan.workName(action.nonce), ExistingWorkPolicy.KEEP, request)
    }

    fun action(data: Data): NotificationAction? = if (data.getString(KIND) == "approve")
        NotificationActionPlan.approval(data.getString(NONCE).orEmpty(), data.getString(SCOPE).orEmpty(), data.getString(COMPUTER).orEmpty(),
            data.getString(BOT).orEmpty(), data.getString(ENTRY).orEmpty(), data.getString(OPTION).orEmpty())
    else NotificationActionPlan.reply(data.getString(NONCE).orEmpty(), data.getString(SCOPE).orEmpty(), data.getString(COMPUTER).orEmpty(),
        data.getString(BOT).orEmpty(), data.getString(MESSAGE).orEmpty())

    private fun data(action: NotificationAction): Data = Data.Builder().putString(KIND, if (action.isApproval) "approve" else "reply")
        .putString(NONCE, action.nonce).putString(SCOPE, action.contextId).putString(COMPUTER, action.computerId).putString(BOT, action.botId)
        .putString(MESSAGE, action.reply).putString(ENTRY, action.entryId).putString(OPTION, action.optionId).build()
}

class WidgetActionWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val action = WidgetActionWork.action(inputData) ?: return Result.failure()
        // The widgets belong to one account; a replaced account never answers for the old one's request.
        val owner = WidgetStore.owner(applicationContext)?.takeIf { it.scope == action.contextId } ?: return Result.failure()
        val local = LocalStore(applicationContext, owner.context)
        val computer = withContext(Dispatchers.IO) { local.computers() }.firstOrNull { it.id == action.computerId } ?: return fail(action)
        if (!local.hasIdentity()) return fail(action)
        var channel: EncryptedChannel? = null
        return try {
            withTimeout(45_000) {
                val connected = HostConnector.connect(computer, withContext(Dispatchers.IO) { local.identity() })
                channel = connected
                connected.state.firstReady()
                if (action.isApproval) connected.call("respondPermission", buildJsonObject {
                    put("botId", action.botId); put("entryId", action.entryId.orEmpty()); put("optionId", action.optionId.orEmpty())
                }) else connected.call("send", buildJsonObject {
                    put("botId", action.botId); put("text", action.reply.orEmpty()); put("clientNonce", action.nonce)
                })
            }
            action.entryId?.let { WidgetStore.sent(applicationContext, it) }
            // The widgets learn the outcome from a fresh read, not from this answer.
            runCatching { WidgetRefreshWorker.refresh(applicationContext) }
            runCatching { CodyncWidgets.update(applicationContext) }
            Result.success()
        } catch (cancelled: CancellationException) {
            if (cancelled is TimeoutCancellationException) retry(action) else throw cancelled
        } catch (error: HostError) {
            if (error.status in listOf(403, 426)) fail(action) else retry(action)
        } catch (_: Exception) {
            retry(action)
        } finally {
            channel?.close()
        }
    }

    private suspend fun retry(action: NotificationAction): Result = if (runAttemptCount < 4) Result.retry() else fail(action)

    /** Says what really happened; the widget stops showing "Sending…" and the notification says the answer didn't go. */
    private suspend fun fail(action: NotificationAction): Result {
        action.entryId?.let { WidgetStore.sent(applicationContext, it) }
        runCatching { CodyncWidgets.update(applicationContext) }
        val text = if (action.isApproval) "Your answer wasn't sent. Tap to review the request." else "Your message wasn't sent. Tap to send it from the chat."
        NotificationDisplay.actionFailed(applicationContext, action.contextId, action.computerId, action.botId, text, action.reply)
        return Result.failure()
    }
}
