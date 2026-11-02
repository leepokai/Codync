package com.codync.android

import android.content.Context
import androidx.work.*
import com.codync.android.core.*
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
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
                val computers = withContext(Dispatchers.IO) { local.computers() }
                if (computers.isEmpty()) return@withTimeout false
                val usageId = computers.currentId(withContext(Dispatchers.IO) { local.lastComputer() })
                val identity = withContext(Dispatchers.IO) { local.identity() }
                // Each computer at once, each in its own 20 s; one that can't be reached keeps the bots it had (and isn't fresh).
                val reads = coroutineScope {
                    computers.map { computer ->
                        async {
                            try { withTimeoutOrNull(20_000) { read(computer, identity, computer.id == usageId) } }
                            catch (cancelled: CancellationException) { throw cancelled }
                            catch (error: Exception) { android.util.Log.i("Codync", "Widget refresh couldn't read ${computer.id}: ${error.message}"); null }
                        }
                    }.awaitAll()
                }
                val previous = withContext(Dispatchers.IO) { WidgetStore.feed(context) }
                val bots = computers.zip(reads).map { (computer, read) ->
                    read?.let { WidgetComputer(it.computer, it.bots, true) } ?: WidgetComputer(computer, null, false)
                }
                val usage = reads.firstNotNullOfOrNull { it?.usage }
                val usageComputer = bots.first { it.computer.id == usageId }.computer
                withContext(Dispatchers.IO) {
                    WidgetStore.publish(context, owner, bots, usageComputer, usage ?: previous.usage.takeIf { previous.computerId == usageId } ?: UsageReport(),
                        usage != null, expectedRevision = revision)
                }
            }
        }
        private class Read(val computer: Computer, val bots: List<Bot>, val usage: UsageReport?)

        /** One computer's roster (and, for the last active one, its usage), then the link closes. */
        private suspend fun read(computer: Computer, identity: DeviceIdentity, usage: Boolean): Read {
            val channel = HostConnector.connect(computer, identity)
            try {
                val current = HostClient.hello(channel, computer)
                // The finite roster snapshot includes its authoritative custom icons.
                val bots = HostClient.bots(channel)
                val report = if (usage) UsageReport.decode(channel.call("usage", buildJsonObject { put("refresh", false) })) else null
                return Read(current, bots, report)
            } finally { channel.close() }
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
