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
        } catch (_: TimeoutCancellationException) { return if (runAttemptCount < 2) Result.retry() else Result.failure() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { return if (runAttemptCount < 2) Result.retry() else Result.failure() }
    }
    companion object {
        private const val NAME = "codync-widget-refresh"
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
            val request = PeriodicWorkRequestBuilder<WidgetRefreshWorker>(30, TimeUnit.MINUTES)
                .setInitialDelay(30, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS).build()
            work.enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.UPDATE, request)
        }
    }
}
