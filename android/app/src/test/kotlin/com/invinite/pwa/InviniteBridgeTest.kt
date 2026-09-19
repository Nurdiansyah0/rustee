package com.invinite.pwa

import android.app.Activity
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Unit tests for InviniteBridge origin validation, versioning,
 * capability boundaries, and JSON dispatch protocol.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 29 (JavaScript <-> Kotlin Bridge)
 * - Restrict JS bridge invocation strictly to trusted application origins.
 * - Versioned capability negotiation ("1.0.0").
 * - Capability dispatch: check_permissions, request_notification_permission, get_device_id, haptic_feedback.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class InviniteBridgeTest {

    private lateinit var activity: Activity
    private lateinit var secureStorage: SecureStorage
    private lateinit var bridge: InviniteBridge

    @Before
    fun setUp() {
        activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        secureStorage = SecureStorage(activity)
        bridge = InviniteBridge(activity, null, secureStorage)
        bridge.setCurrentUrl("https://api.nurdiansyahlabs.com")
    }

    @Test
    fun testAuthorizedOriginsAccepted() {
        val authorizedUrls = listOf(
            "https://api.nurdiansyahlabs.com",
            "https://api.nurdiansyahlabs.com/",
            "https://api.nurdiansyahlabs.com:443",
            "https://api.nurdiansyahlabs.com:443/dashboard",
            "https://api.nurdiansyahlabs.com/dashboard",
            "https://app.nurdiansyahlabs.com",
            "https://app.nurdiansyahlabs.com/transactions",
            "https://nurdiansyahlabs.com",
            "http://localhost:5173",
            "http://localhost:5173/",
            "http://127.0.0.1:5173",
            "http://10.0.2.2:5173",
            "file:///android_asset/index.html"
        )

        for (url in authorizedUrls) {
            assertTrue("Expected URL $url to be authorized", InviniteBridge.isAuthorizedOrigin(url))
        }
    }

    @Test
    fun testUnauthorizedOriginsStrictlyRejected() {
        val unauthorizedUrls = listOf(
            "https://malicious.com",
            "http://evil.org",
            "https://api.nurdiansyahlabs.com.attacker.com",
            "https://attacker-api.nurdiansyahlabs.com",
            "http://api.nurdiansyahlabs.com", // Plain HTTP prohibited for remote domain
            "http://subdomain.nurdiansyahlabs.com",
            "https://api.nurdiansyahlabs.com:8443", // Non-standard port on remote domain
            "https://attacker@api.nurdiansyahlabs.com", // Userinfo prohibited
            "https://user:pass@api.nurdiansyahlabs.com/dashboard",
            "javascript:alert(1)",
            "data:text/html,<html></html>",
            "ftp://files.example.com",
            "",
            "   ",
            null
        )

        for (url in unauthorizedUrls) {
            assertFalse("Expected URL $url to be rejected", InviniteBridge.isAuthorizedOrigin(url))
        }
    }

    @Test
    fun testBridgeVersionConstant() {
        assertEquals("1.0.0", InviniteBridge.BRIDGE_VERSION)
        assertEquals("1.0.0", bridge.getVersion())
    }

    @Test
    fun testPostMessageCheckPermissions() {
        val requestJson = JSONObject().apply {
            put("action", "check_permissions")
        }.toString()

        val responseJson = bridge.postMessage(requestJson)
        assertNotNull(responseJson)

        val response = JSONObject(responseJson)
        assertEquals("check_permissions", response.getString("action"))
        assertTrue(response.has("notifications"))
        assertTrue(response.has("notification_listener"))
        assertTrue(response.has("network"))
        assertEquals("1.0.0", response.getString("version"))
    }

    @Test
    fun testPostMessageRequestNotificationPermission() {
        val requestJson = JSONObject().apply {
            put("action", "request_notification_permission")
        }.toString()

        val responseJson = bridge.postMessage(requestJson)
        val response = JSONObject(responseJson)
        assertEquals("request_notification_permission", response.getString("action"))
        assertEquals("requested", response.getString("status"))
    }

    @Test
    fun testPostMessageGetDeviceId() {
        val requestJson = JSONObject().apply {
            put("action", "get_device_id")
        }.toString()

        val responseJson = bridge.postMessage(requestJson)
        val response = JSONObject(responseJson)
        assertEquals("get_device_id", response.getString("action"))

        val deviceId = response.getString("device_id")
        assertTrue("Device ID should be UUID format", deviceId.matches(Regex("^[0-9a-fA-F-]{36}$")))
        assertEquals(secureStorage.getDeviceId(), deviceId)
    }

    @Test
    fun testPostMessageHapticFeedbackStyles() {
        for (style in listOf("light", "medium", "heavy", "custom")) {
            val requestJson = JSONObject().apply {
                put("action", "haptic_feedback")
                put("style", style)
            }.toString()

            val responseJson = bridge.postMessage(requestJson)
            val response = JSONObject(responseJson)
            assertEquals("haptic_feedback", response.getString("action"))
            assertEquals(style, response.getString("style"))
            assertEquals("executed", response.getString("status"))
        }
    }

    @Test
    fun testPostMessageGetBridgeVersion() {
        val requestJson = JSONObject().apply {
            put("action", "get_bridge_version")
        }.toString()

        val responseJson = bridge.postMessage(requestJson)
        val response = JSONObject(responseJson)
        assertEquals("get_bridge_version", response.getString("action"))
        assertEquals("1.0.0", response.getString("version"))
    }

    @Test
    fun testPostMessageScheduleSync() {
        val requestJson = JSONObject().apply {
            put("action", "schedule_sync")
        }.toString()

        val responseJson = bridge.postMessage(requestJson)
        val response = JSONObject(responseJson)
        assertEquals("schedule_sync", response.getString("action"))
        assertEquals("scheduled", response.getString("status"))
    }

    @Test
    fun testPostMessageUnsupportedActionRejected() {
        val requestJson = JSONObject().apply {
            put("action", "arbitrary_eval_execution")
        }.toString()

        val responseJson = bridge.postMessage(requestJson)
        val response = JSONObject(responseJson)
        assertEquals("CAPABILITY_UNSUPPORTED", response.getString("code"))
        assertTrue(response.getString("error").contains("Unsupported capability action"))
    }

    @Test
    fun testPostMessageMalformedJsonHandledGracefully() {
        val responseJson = bridge.postMessage("NOT_VALID_JSON{{{")
        val response = JSONObject(responseJson)
        assertEquals("BRIDGE_EXECUTION_ERROR", response.getString("code"))
        assertTrue(response.has("error"))
    }

    @Test
    fun testPostMessageUnauthorizedOriginStrictlyRejectedWith403() {
        bridge.setCurrentUrl("https://evil-attacker.com/steal-data")

        val requestJson = JSONObject().apply {
            put("action", "get_device_id")
        }.toString()

        val responseJson = bridge.postMessage(requestJson)
        val response = JSONObject(responseJson)

        assertEquals("UNAUTHORIZED_ORIGIN", response.getString("code"))
        assertEquals(403, response.getInt("status"))
        assertEquals("Unauthorized origin", response.getString("error"))
    }

    @Test
    fun testDirectMethodInvocationOriginEnforcement() {
        bridge.setCurrentUrl("https://evil-attacker.com")

        // Direct checkPermissions returns error JSON
        val permResult = bridge.checkPermissions()
        assertTrue("Expected unauthorized error in permResult", permResult.contains("Unauthorized origin"))

        // Direct getDeviceId returns empty string
        val deviceId = bridge.getDeviceId()
        assertEquals("", deviceId)

        // Switch to authorized origin -> success
        bridge.setCurrentUrl("https://api.nurdiansyahlabs.com")
        val validDeviceId = bridge.getDeviceId()
        assertTrue(validDeviceId.isNotEmpty())
    }
}
