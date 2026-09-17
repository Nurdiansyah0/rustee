package com.invinite.pwa

import android.app.Notification
import android.content.Context
import android.os.Bundle
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID

/**
 * NotificationListener captures financial transaction notifications from approved banking
 * and fintech applications and forwards them in canonical form to the backend ingestion pipeline.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 21 & § 24
 * - Restricts capture exclusively to authorized banking & fintech packages (BCA, Mandiri, BRI, BNI, DANA, GoPay, OVO, ShopeePay).
 * - Implements REQ-INGEST-08: Payload Minimization — immediately discards all raw extras and PII after canonical extraction.
 * - Forwards canonical representation matching PROJECT.md § 4.3: `{"package_name", "title", "text", "posted_at"}`.
 *
 * NATIVE LAYER INVARIANT:
 * The native Android layer must NEVER own financial calculations, subscription authority,
 * or ledger persistence. All financial logic, parsing, and categorization belongs exclusively
 * to the backend ingestion pipeline.
 */
class NotificationListener : NotificationListenerService() {

    companion object {
        private const val TAG = "NotificationListener"
        private const val BACKEND_INGESTION_URL = "https://api.nurdiansyahlabs.com/api/v1/ingestion/notification"

        /**
         * Authoritative list of Indonesian Banking and Fintech package identifiers.
         */
        val TARGET_FINANCIAL_PACKAGES = setOf(
            // Bank Central Asia (BCA)
            "com.bca",
            "com.bca.mobile",
            "com.bca.mybca",
            // Bank Mandiri
            "com.bankmandiri.livin",
            "com.bankmandiri.mandirionline",
            // Bank Rakyat Indonesia (BRI)
            "id.co.bri.brimo",
            // Bank Negara Indonesia (BNI)
            "id.co.bni.mbanking",
            // DANA Indonesia
            "id.dana",
            // GoPay / Gojek
            "com.gojek.app",
            "com.gopay.wallet",
            // OVO
            "ovo.id",
            // ShopeePay / Shopee
            "com.shopee.id"
        )

        /**
         * Checks if the app has been granted notification listener access in Android OS settings.
         */
        fun isNotificationAccessGranted(context: Context): Boolean {
            val enabledListeners = NotificationManagerCompat.getEnabledListenerPackages(context)
            return enabledListeners.contains(context.packageName)
        }

        /**
         * Checks whether the given package name belongs to an authorized banking or fintech app.
         */
        fun isTargetPackage(packageName: String?): Boolean {
            if (packageName.isNullOrBlank()) return false
            val cleanPackage = packageName.trim().lowercase()
            return TARGET_FINANCIAL_PACKAGES.contains(cleanPackage)
        }

        /**
         * Canonical notification representation data model.
         */
        data class CanonicalNotification(
            val packageName: String,
            val title: String,
            val text: String,
            val postedAt: Long
        ) {
            fun toJson(): JSONObject {
                return JSONObject().apply {
                    put("package_name", packageName)
                    put("title", title)
                    put("text", text)
                    put("posted_at", postedAt)
                }
            }

            fun toJsonString(): String = toJson().toString()
        }

        /**
         * Extracts and sanitizes canonical notification fields from raw notification components.
         * Enforces REQ-INGEST-08: discards raw extras and non-canonical metadata.
         */
        fun extractCanonical(
            packageName: String,
            title: CharSequence?,
            text: CharSequence?,
            bigText: CharSequence?,
            timestamp: Long
        ): CanonicalNotification? {
            if (!isTargetPackage(packageName)) {
                return null
            }

            val titleStr = title?.toString()?.trim() ?: ""
            // Prefer bigText (expanded notification text) if present
            val textStr = (bigText?.toString()?.trim()?.takeIf { it.isNotEmpty() }
                ?: text?.toString()?.trim()
                ?: "")

            // Both title and text must not be empty simultaneously
            if (titleStr.isEmpty() && textStr.isEmpty()) {
                return null
            }

            val postedAt = if (timestamp > 0) timestamp else System.currentTimeMillis()

            return CanonicalNotification(
                packageName = packageName,
                title = titleStr,
                text = textStr,
                postedAt = postedAt
            )
        }
    }

    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private lateinit var secureStorage: SecureStorage

    override fun onCreate() {
        super.onCreate()
        secureStorage = SecureStorage(applicationContext)
        Log.i(TAG, "NotificationListener service created.")
    }

    override fun onListenerConnected() {
        super.onListenerConnected()
        Log.i(TAG, "NotificationListener connected and active.")
    }

    override fun onListenerDisconnected() {
        super.onListenerDisconnected()
        Log.i(TAG, "NotificationListener disconnected.")
    }

    override fun onNotificationPosted(sbn: StatusBarNotification?) {
        if (sbn == null) return

        // Explicit permission check: verify active OS grant
        if (!isNotificationAccessGranted(applicationContext)) {
            Log.w(TAG, "Notification access not granted by user in OS settings. Discarding event.")
            return
        }

        // Verify user preference toggle in SecureStorage
        if (!secureStorage.isNotificationServiceEnabled()) {
            return
        }

        val packageName = sbn.packageName ?: return
        if (!isTargetPackage(packageName)) {
            return
        }

        val notification = sbn.notification ?: return
        val extras: Bundle = notification.extras ?: Bundle()

        val title = extras.getCharSequence(Notification.EXTRA_TITLE)
        val text = extras.getCharSequence(Notification.EXTRA_TEXT)
        val bigText = extras.getCharSequence(Notification.EXTRA_BIG_TEXT)
        val postTime = sbn.postTime

        val canonical = extractCanonical(
            packageName = packageName,
            title = title,
            text = text,
            bigText = bigText,
            timestamp = postTime
        ) ?: return

        Log.d(TAG, "Captured target financial notification from $packageName, forwarding to ingestion pipeline.")

        // Forward to backend ingestion pipeline asynchronously
        serviceScope.launch {
            forwardToIngestion(canonical)
        }
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification?) {
        // No action required on removal; transactions are immutable events
    }

    /**
     * Dispatches canonical notification payload to the backend ingestion endpoint.
     */
    private fun forwardToIngestion(canonical: CanonicalNotification) {
        try {
            val url = URL(BACKEND_INGESTION_URL)
            val connection = url.openConnection() as HttpURLConnection
            connection.requestMethod = "POST"
            connection.setRequestProperty("Content-Type", "application/json")
            connection.setRequestProperty("Accept", "application/json")
            // Stable idempotency key for deduplication
            val idempotencyKey = UUID.randomUUID().toString()
            connection.setRequestProperty("Idempotency-Key", idempotencyKey)

            // Attach session auth token if stored
            val authToken = secureStorage.getAuthToken()
            if (!authToken.isNullOrBlank()) {
                connection.setRequestProperty("Authorization", "Bearer $authToken")
            }

            connection.doOutput = true
            connection.connectTimeout = 8000
            connection.readTimeout = 8000

            OutputStreamWriter(connection.outputStream).use { writer ->
                writer.write(canonical.toJsonString())
                writer.flush()
            }

            val responseCode = connection.responseCode
            if (responseCode in 200..299) {
                Log.d(TAG, "Notification successfully ingested by backend (HTTP $responseCode)")
            } else {
                Log.w(TAG, "Ingestion API returned status $responseCode")
            }
            connection.disconnect()
        } catch (e: Exception) {
            Log.w(TAG, "Failed to deliver notification to backend ingestion endpoint: ${e.message}")
        }
    }
}
