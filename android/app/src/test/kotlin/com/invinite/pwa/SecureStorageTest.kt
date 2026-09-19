package com.invinite.pwa

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Unit tests for SecureStorage encryption, device ID stability,
 * token lifecycle, and cursor persistence.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class SecureStorageTest {

    private lateinit var context: Context
    private lateinit var secureStorage: SecureStorage

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        secureStorage = SecureStorage(context)
    }

    @Test
    fun testDeviceIdGenerationAndPersistence() {
        val deviceId1 = secureStorage.getDeviceId()
        assertNotNull("Device ID must not be null", deviceId1)
        assertTrue("Device ID must be valid UUID string", deviceId1.matches(Regex("^[0-9a-fA-F-]{36}$")))

        val deviceId2 = secureStorage.getDeviceId()
        assertEquals("Device ID must be stable and persistent across calls", deviceId1, deviceId2)
    }

    @Test
    fun testAuthTokenStorageAndClearing() {
        val sampleToken = "jwt.header.payload.signature"
        secureStorage.saveAuthToken(sampleToken)
        assertEquals(sampleToken, secureStorage.getAuthToken())

        secureStorage.clearAuthToken()
        assertEquals(null, secureStorage.getAuthToken())
    }

    @Test
    fun testSyncCursorPersistence() {
        secureStorage.saveSyncCursor(42L)
        assertEquals(42L, secureStorage.getSyncCursor())

        secureStorage.saveSyncCursor(100L)
        assertEquals(100L, secureStorage.getSyncCursor())
    }

    @Test
    fun testNotificationServiceEnabledToggle() {
        // Defaults to true
        assertTrue(secureStorage.isNotificationServiceEnabled())

        secureStorage.setNotificationServiceEnabled(false)
        assertFalse(secureStorage.isNotificationServiceEnabled())

        secureStorage.setNotificationServiceEnabled(true)
        assertTrue(secureStorage.isNotificationServiceEnabled())
    }

    @Test
    fun testGenericStringStorage() {
        secureStorage.putString("custom_pref", "custom_val")
        assertEquals("custom_val", secureStorage.getString("custom_pref"))
        assertEquals("default", secureStorage.getString("non_existent", "default"))
    }

    @Test
    fun testClearSessionPreservesDeviceId() {
        val originalDeviceId = secureStorage.getDeviceId()
        secureStorage.saveAuthToken("token-to-be-cleared")
        secureStorage.putString("temp_setting", "setting_val")

        secureStorage.clearSession()

        // Auth token and generic settings must be cleared
        assertEquals(null, secureStorage.getAuthToken())
        assertEquals(null, secureStorage.getString("temp_setting"))

        // Device ID must be preserved
        assertEquals(originalDeviceId, secureStorage.getDeviceId())
    }
}
