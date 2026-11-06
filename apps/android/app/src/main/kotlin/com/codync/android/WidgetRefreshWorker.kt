package com.codync.android

import android.content.Context
import androidx.work.*
import com.codync.android.core.*
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.*

/** Bounded, short-lived host queries. System scheduling can delay every refresh. */
class WidgetRefreshWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        try {
            refresh(applicationContext)
            CodyncWidgets.update(applicationContext)
            return Result.success()
        } catch (timeout: TimeoutCancellationException) { return failed() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { return failed() }
    }

    /** The widgets say they are offline, rather than keep showing the last numbers as if they were current. */
    private suspend fun failed(): Result {
        WidgetStore.owner(applicationContext)?.let { WidgetStore.markOffline(applicationContext, it) }
        runCatching { CodyncWidgets.update(applicationContext) }
        return if (runAttemptCount < 2) Result.retry() else Result.failure()
    }

    companion object {
        private const val NAME = "codync-widget-refresh"
        /** WorkManager's shortest period; while the app runs, its events refresh the widgets sooner (`AppStore.observeWidgets`). */
        private const val PERIOD_MINUTES = 15L
        private const val FLEX_MINUTES = 5L
        private val gate = Mutex()
        internal suspend fun refresh(context: Context): Boolean = gate.withLock {
            val owner = WidgetStore.owner(context) ?: return@withLock false
            val revision = WidgetStore.changes.value
            withTimeout(30_000) {
                val local = LocalStore(context, owner.context)
                val computer = withContext(Dispatchers.IO) { local.computers().firstOrNull() } ?: return@withTimeout false
                val identity = withContext(Dispatchers.IO) { local.identity() }
                val channel = HostConnector.connect(computer, identity)
                try {
                    val current = HostClient.hello(channel, computer)
                    val bots = HostClient.bots(channel)
                    val response = channel.call("usage", buildJsonObject { put("refresh", false) })
                    val usage = UsageReport.decode(response)
                    withContext(Dispatchers.IO) { WidgetStore.publish(context, owner, current, bots, usage, true, true, expectedRevision = revision) }
                } finally { channel.close() }
            }
        }
        suspend fun stop(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(NAME)
            // Join every reader before account erasure can remove its storage.
            gate.withLock { }
        }
        fun schedule(context: Context, paired: Boolean) {
            val work = WorkManager.getInstance(context)
            if (!paired || CodyncWidgets.installed(context) == 0) { work.cancelUniqueWork(NAME); return }
            val request = PeriodicWorkRequestBuilder<WidgetRefreshWorker>(PERIOD_MINUTES, TimeUnit.MINUTES, FLEX_MINUTES, TimeUnit.MINUTES)
                .setInitialDelay(PERIOD_MINUTES, TimeUnit.MINUTES)
                // Battery saver and a low battery postpone the read: the widgets then say how old they are.
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).setRequiresBatteryNotLow(true).build())
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS).build()
            work.enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.UPDATE, request)
        }
    }
}
