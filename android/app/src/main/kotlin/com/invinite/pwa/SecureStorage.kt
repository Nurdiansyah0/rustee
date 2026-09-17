package com.invinite.pwa

import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import java.util.UUID

/**
 * SecureStorage manages encrypted local key-value persistence for the native Android shell.
 * Uses EncryptedSharedPreferences backed by Android Keystore (AES-256 GCM).
 *
 * NATIVE LAYER INVARIANT:
 * The native Android layer must NEVER own financial calculations, subscription authority,
 * or ledger persistence. All financial logic belongs exclusively to the backend.
 * This storage is restricted to non-sensitive device installation identifiers, session tokens,
 * and sync coordination metadata.
 */
class SecureStorage(private val context: Context) {

    companion object {
        private const val TAG = "SecureStorage"
        private const val PREFS_FILE = "invinite_secure_prefs"
        private const val KEY_DEVICE_ID = "device_installation_id"
        private const val KEY_AUTH_TOKEN = "auth_session_token"
        private const val KEY_SYNC_CURSOR = "last_sync_cursor"
        private const val KEY_NOTIFICATION_ENABLED = "notification_listener_enabled"
    }

    private val prefs: SharedPreferences by lazy {
        initPreferences()
    }

    private fun initPreferences(): SharedPreferences {
        return try {
            val masterKey = MasterKey.Builder(context)
                .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                .build()

            EncryptedSharedPreferences.create(
                context,
                PREFS_FILE,
                masterKey,
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
            )
        } catch (e: Exception) {
            // Graceful fallback for test environments or Keystore corruption
            Log.w(TAG, "EncryptedSharedPreferences initialization failed, falling back to standard prefs: ${e.message}")
            context.getSharedPreferences(PREFS_FILE + "_fallback", Context.MODE_PRIVATE)
        }
    }

    /**
     * Retrieves or generates a stable, pseudo-anonymous installation UUID.
     * Never exposes hardware PII (IMEI, MAC, serial).
     */
    fun getDeviceId(): String {
        var deviceId = prefs.getString(KEY_DEVICE_ID, null)
        if (deviceId.isNullOrBlank()) {
            deviceId = UUID.randomUUID().toString()
            prefs.edit().putString(KEY_DEVICE_ID, deviceId).apply()
        }
        return deviceId
    }

    /**
     * Stores session authorization token securely.
     */
    fun saveAuthToken(token: String) {
        prefs.edit().putString(KEY_AUTH_TOKEN, token).apply()
    }

    /**
     * Retrieves stored session authorization token.
     */
    fun getAuthToken(): String? {
        return prefs.getString(KEY_AUTH_TOKEN, null)
    }

    /**
     * Clears stored authorization token on logout.
     */
    fun clearAuthToken() {
        prefs.edit().remove(KEY_AUTH_TOKEN).apply()
    }

    /**
     * Stores last known cursor for delta sync coordination.
     */
    fun saveSyncCursor(cursor: Long) {
        prefs.edit().putLong(KEY_SYNC_CURSOR, cursor).apply()
    }

    /**
     * Retrieves last known sync cursor (defaults to 0).
     */
    fun getSyncCursor(): Long {
        return prefs.getLong(KEY_SYNC_CURSOR, 0L)
    }

    /**
     * Saves generic string property.
     */
    fun putString(key: String, value: String) {
        prefs.edit().putString(key, value).apply()
    }

    /**
     * Retrieves generic string property.
     */
    fun getString(key: String, defaultValue: String? = null): String? {
        return prefs.getString(key, defaultValue)
    }

    /**
     * Records user notification preference flag.
     */
    fun setNotificationServiceEnabled(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_NOTIFICATION_ENABLED, enabled).apply()
    }

    /**
     * Checks user notification preference flag.
     */
    fun isNotificationServiceEnabled(): Boolean {
        return prefs.getBoolean(KEY_NOTIFICATION_ENABLED, true)
    }

    /**
     * Clears all non-device preferences (e.g., upon user logout or factory reset).
     * Preserves device installation ID.
     */
    fun clearSession() {
        val deviceId = getDeviceId()
        prefs.edit().clear().apply()
        prefs.edit().putString(KEY_DEVICE_ID, deviceId).apply()
    }
}
