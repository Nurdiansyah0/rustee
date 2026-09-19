package com.invinite.pwa

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.provider.Settings
import android.util.Log
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject

/**
 * InviniteBridge provides a strictly capability-based, origin-validated JavaScript bridge
 * injected into the Android WebView as `window.InviniteBridge`.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 29 (JavaScript <-> Kotlin Bridge)
 * - Restricts execution exclusively to authorized origins (https://api.nurdiansyahlabs.com and authorized dev hosts).
 * - Exposes only validated capabilities: check_permissions, request_notification_permission, get_device_id, haptic_feedback.
 * - Prohibits arbitrary code execution, arbitrary networking, and file access.
 *
 * NATIVE LAYER INVARIANT:
 * The native layer must NEVER own financial calculations, subscription authority,
 * or ledger persistence. All financial logic belongs exclusively to the backend.
 */
class InviniteBridge(
    private val activity: Activity,
    private val webView: WebView? = null,
    private val secureStorage: SecureStorage
) {

    companion object {
        private const val TAG = "InviniteBridge"
        const val BRIDGE_VERSION = "1.0.0"
        const val PERMISSION_REQUEST_CODE_NOTIFICATIONS = 1001

        // Strict whitelist of authorized hosts
        private val AUTHORIZED_HOSTS = setOf(
            "api.nurdiansyahlabs.com",
            "app.nurdiansyahlabs.com",
            "nurdiansyahlabs.com",
            "localhost",
            "127.0.0.1",
            "10.0.2.2"
        )

        /**
         * Validates whether a given URL is within the authorized origins whitelist.
         * Enforces scheme, host, userinfo, and port constraints.
         */
        fun isAuthorizedOrigin(url: String?): Boolean {
            if (url.isNullOrBlank()) return false
            val cleanUrl = url.trim()

            // Allow bundled offline assets
            if (cleanUrl.startsWith("file:///android_asset/")) {
                return true
            }

            return try {
                val uri = java.net.URI(cleanUrl)
                val scheme = uri.scheme?.lowercase() ?: return false
                val host = uri.host?.lowercase() ?: return false

                // Disallow userinfo to prevent phishing/credential spoofing
                if (uri.userInfo != null) {
                    return false
                }

                if (scheme != "https" && scheme != "http") {
                    return false
                }

                // In production, remote hosts MUST use HTTPS
                if (scheme == "http" && host != "localhost" && host != "127.0.0.1" && host != "10.0.2.2") {
                    return false
                }

                // For remote production hosts, only standard HTTPS port (443 or -1) is allowed
                if (scheme == "https" && uri.port != -1 && uri.port != 443) {
                    return false
                }

                AUTHORIZED_HOSTS.contains(host)
            } catch (e: Exception) {
                false
            }
        }
    }

    @Volatile
    private var trackedUrl: String? = null

    /**
     * Updates the currently validated active URL (called on page load / navigation).
     */
    fun setCurrentUrl(url: String?) {
        trackedUrl = url
    }

    /**
     * Resolves the current URL from tracking or directly from WebView.
     */
    fun getCurrentUrl(): String? {
        return trackedUrl ?: try {
            webView?.url
        } catch (e: Exception) {
            null
        }
    }

    /**
     * Checks if current active URL is from an authorized origin.
     */
    fun validateOrigin(): Boolean {
        val url = getCurrentUrl()
        val authorized = isAuthorizedOrigin(url)
        if (!authorized) {
            Log.w(TAG, "Security Alert: Rejected bridge call from unauthorized origin: $url")
        }
        return authorized
    }

    /**
     * Core dispatch protocol message handler complying with PROJECT.md § 4.5:
     * `interface InviniteBridge { postMessage(message: string): void; }`
     */
    @JavascriptInterface
    fun postMessage(messageJson: String): String {
        if (!validateOrigin()) {
            return JSONObject().apply {
                put("error", "Unauthorized origin")
                put("code", "UNAUTHORIZED_ORIGIN")
                put("status", 403)
            }.toString()
        }

        return try {
            val payload = JSONObject(messageJson)
            val action = payload.optString("action", "")

            when (action) {
                "check_permissions" -> {
                    checkPermissions()
                }
                "request_notification_permission" -> {
                    requestNotificationPermission()
                    JSONObject().apply {
                        put("action", action)
                        put("status", "requested")
                    }.toString()
                }
                "get_device_id" -> {
                    JSONObject().apply {
                        put("action", action)
                        put("device_id", getDeviceId())
                    }.toString()
                }
                "haptic_feedback" -> {
                    val style = payload.optString("style", "light")
                    hapticFeedback(style)
                    JSONObject().apply {
                        put("action", action)
                        put("style", style)
                        put("status", "executed")
                    }.toString()
                }
                "get_bridge_version" -> {
                    JSONObject().apply {
                        put("action", action)
                        put("version", BRIDGE_VERSION)
                    }.toString()
                }
                "schedule_sync" -> {
                    try {
                        BackgroundSyncWorker.scheduleOneTimeSync(activity.applicationContext)
                    } catch (e: Exception) {
                        Log.w(TAG, "WorkManager sync scheduling caught: ${e.message}")
                    }
                    JSONObject().apply {
                        put("action", action)
                        put("status", "scheduled")
                    }.toString()
                }
                else -> {
                    Log.w(TAG, "Unsupported capability action requested: $action")
                    JSONObject().apply {
                        put("error", "Unsupported capability action: $action")
                        put("code", "CAPABILITY_UNSUPPORTED")
                        put("action", action)
                    }.toString()
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Bridge message execution failed", e)
            JSONObject().apply {
                put("error", e.message ?: "Execution error")
                put("code", "BRIDGE_EXECUTION_ERROR")
            }.toString()
        }
    }

    /**
     * Inspects active OS permission states.
     * Returns JSON string with `notifications`, `notification_listener`, and `network`.
     */
    @JavascriptInterface
    fun checkPermissions(): String {
        if (!validateOrigin()) {
            return "{\"error\":\"Unauthorized origin\"}"
        }

        val hasNotificationPost = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            ContextCompat.checkSelfPermission(
                activity,
                Manifest.permission.POST_NOTIFICATIONS
            ) == PackageManager.PERMISSION_GRANTED
        } else {
            NotificationManagerCompat.from(activity).areNotificationsEnabled()
        }

        val hasNotificationListener = NotificationListener.isNotificationAccessGranted(activity)
        val hasNetwork = isNetworkConnected()

        return JSONObject().apply {
            put("action", "check_permissions")
            put("notifications", hasNotificationPost)
            put("notification_listener", hasNotificationListener)
            put("network", hasNetwork)
            put("version", BRIDGE_VERSION)
        }.toString()
    }

    /**
     * Triggers permission request for POST_NOTIFICATIONS (Android 13+)
     * or directs user to Notification Listener Settings screen if notification access is required.
     */
    @JavascriptInterface
    fun requestNotificationPermission() {
        if (!validateOrigin()) return

        activity.runOnUiThread {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                if (ContextCompat.checkSelfPermission(
                        activity,
                        Manifest.permission.POST_NOTIFICATIONS
                    ) != PackageManager.PERMISSION_GRANTED
                ) {
                    ActivityCompat.requestPermissions(
                        activity,
                        arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                        PERMISSION_REQUEST_CODE_NOTIFICATIONS
                    )
                    return@runOnUiThread
                }
            }

            // If notification listener is not enabled, guide user to system settings
            if (!NotificationListener.isNotificationAccessGranted(activity)) {
                try {
                    val intent = Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS).apply {
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    activity.startActivity(intent)
                } catch (e: Exception) {
                    Log.e(TAG, "Failed to open notification listener settings", e)
                }
            }
        }
    }

    /**
     * Returns the persistent installation Device ID stored in SecureStorage.
     * Never exposes hardware PII.
     */
    @JavascriptInterface
    fun getDeviceId(): String {
        if (!validateOrigin()) return ""
        return secureStorage.getDeviceId()
    }

    /**
     * Triggers tactile feedback for POS keypad and interactive UI buttons.
     * Styles supported: "light" (keypad tap), "medium" (submit), "heavy" (delete/reset).
     */
    @JavascriptInterface
    fun hapticFeedback(style: String) {
        if (!validateOrigin()) return

        activity.runOnUiThread {
            try {
                val vibrator = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    val vibratorManager = activity.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager
                    vibratorManager?.defaultVibrator
                } else {
                    @Suppress("DEPRECATION")
                    activity.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
                }

                if (vibrator != null && vibrator.hasVibrator()) {
                    val durationMs = when (style.lowercase()) {
                        "heavy" -> 50L
                        "medium" -> 25L
                        "light" -> 10L
                        else -> 10L
                    }

                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                        val amplitude = when (style.lowercase()) {
                            "heavy" -> VibrationEffect.DEFAULT_AMPLITUDE
                            "medium" -> 160
                            "light" -> 80
                            else -> 80
                        }
                        vibrator.vibrate(
                            VibrationEffect.createOneShot(durationMs, amplitude)
                        )
                    } else {
                        @Suppress("DEPRECATION")
                        vibrator.vibrate(durationMs)
                    }
                }
            } catch (e: Exception) {
                Log.w(TAG, "Haptic feedback vibration failed: ${e.message}")
            }
        }
    }

    /**
     * Version accessor for capability negotiation.
     */
    @JavascriptInterface
    fun getVersion(): String {
        return BRIDGE_VERSION
    }

    /**
     * Checks if device currently has active network connectivity.
     */
    private fun isNetworkConnected(): Boolean {
        return try {
            val cm = activity.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager ?: return false
            val network = cm.activeNetwork ?: return false
            val capabilities = cm.getNetworkCapabilities(network) ?: return false
            capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        } catch (e: Exception) {
            false
        }
    }
}
