package com.invinite.pwa

import android.content.Context
import android.util.Log
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.TimeUnit

/**
 * BackgroundSyncWorker coordinates OS-compatible background synchronization via Android WorkManager.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 25 (Realtime & Synchronization) & § 27 (Performance and Battery)
 * - Restricts background sync to connected networks and healthy battery levels.
 * - Performs cursor delta synchronization against `/api/v1/sync?cursor=<cursor>`.
 * - Employs bounded retries with exponential backoff; does not maintain persistent background connections.
 *
 * NATIVE LAYER INVARIANT:
 * The native Android layer must NEVER own financial calculations, subscription authority,
 * or ledger persistence. All financial calculations and ledger states belong to the backend.
 */
class BackgroundSyncWorker(
    appContext: Context,
    workerParams: WorkerParameters
) : CoroutineWorker(appContext, workerParams) {

    companion object {
        private const val TAG = "BackgroundSyncWorker"
        const val PERIODIC_WORK_NAME = "invinite_periodic_sync"
        const val ONE_TIME_WORK_NAME = "invinite_one_time_sync"
        private const val BASE_SYNC_URL = "https://api.nurdiansyahlabs.com/api/v1/sync"

        /**
         * Schedules periodic background delta sync (defaults to 15-minute intervals).
         */
        fun schedulePeriodicSync(context: Context, intervalMinutes: Long = 15) {
            val constraints = Constraints.Builder()
                .setRequiredNetworkType(NetworkType.CONNECTED)
                .setRequiresBatteryNotLow(true)
                .build()

            val syncRequest = PeriodicWorkRequestBuilder<BackgroundSyncWorker>(
                intervalMinutes,
                TimeUnit.MINUTES
            )
                .setConstraints(constraints)
                .build()

            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                PERIODIC_WORK_NAME,
                ExistingPeriodicWorkPolicy.KEEP,
                syncRequest
            )
            Log.d(TAG, "Periodic sync enqueued with interval $intervalMinutes minutes.")
        }

        /**
         * Triggers an immediate one-time background sync request.
         */
        fun scheduleOneTimeSync(context: Context) {
            val constraints = Constraints.Builder()
                .setRequiredNetworkType(NetworkType.CONNECTED)
                .build()

            val syncRequest = OneTimeWorkRequestBuilder<BackgroundSyncWorker>()
                .setConstraints(constraints)
                .build()

            WorkManager.getInstance(context).enqueueUniqueWork(
                ONE_TIME_WORK_NAME,
                ExistingWorkPolicy.REPLACE,
                syncRequest
            )
            Log.d(TAG, "One-time sync scheduled.")
        }

        /**
         * Cancels all scheduled background sync tasks.
         */
        fun cancelAllSync(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(PERIODIC_WORK_NAME)
            WorkManager.getInstance(context).cancelUniqueWork(ONE_TIME_WORK_NAME)
            Log.d(TAG, "All background sync tasks cancelled.")
        }
    }

    private val secureStorage = SecureStorage(applicationContext)

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val currentCursor = secureStorage.getSyncCursor()
        Log.i(TAG, "Starting background sync with cursor $currentCursor (run attempt $runAttemptCount)")

        val authToken = secureStorage.getAuthToken()
        // If not authenticated, skip sync until next login
        if (authToken.isNullOrBlank()) {
            Log.i(TAG, "No authentication token present; skipping background sync.")
            return@withContext Result.success()
        }

        try {
            val syncUrl = URL("$BASE_SYNC_URL?cursor=$currentCursor")
            val connection = syncUrl.openConnection() as HttpURLConnection
            connection.requestMethod = "GET"
            connection.setRequestProperty("Accept", "application/json")
            connection.setRequestProperty("Authorization", "Bearer $authToken")
            connection.connectTimeout = 10000
            connection.readTimeout = 10000

            val responseCode = connection.responseCode
            if (responseCode == HttpURLConnection.HTTP_OK) {
                val responseText = connection.inputStream.bufferedReader().use(BufferedReader::readText)
                val json = JSONObject(responseText)

                val newCursor = json.optLong("cursor", currentCursor)
                if (newCursor > currentCursor) {
                    secureStorage.saveSyncCursor(newCursor)
                    Log.i(TAG, "Sync successful. Cursor advanced from $currentCursor to $newCursor.")
                } else {
                    Log.i(TAG, "Sync complete. No new updates; cursor remained at $currentCursor.")
                }

                connection.disconnect()
                Result.success()
            } else if (responseCode == HttpURLConnection.HTTP_UNAUTHORIZED) {
                Log.w(TAG, "Auth token expired or invalid during background sync (HTTP 401).")
                secureStorage.clearAuthToken()
                connection.disconnect()
                Result.failure()
            } else {
                Log.w(TAG, "Sync API returned status $responseCode")
                connection.disconnect()
                if (runAttemptCount < 3) {
                    Result.retry()
                } else {
                    Result.failure()
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "Background sync failed: ${e.message}")
            if (runAttemptCount < 3) {
                Result.retry()
            } else {
                Result.failure()
            }
        }
    }
}
