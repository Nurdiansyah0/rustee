package com.invinite.pwa

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for InviniteBridge origin validation, versioning,
 * and capability boundary verification.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 29 (JavaScript <-> Kotlin Bridge)
 * - Restrict JS bridge invocation strictly to trusted application origins.
 * - Versioned capability negotiation ("1.0.0").
 */
class InviniteBridgeTest {

    @Test
    fun testAuthorizedOriginsAccepted() {
        val authorizedUrls = listOf(
            "https://api.nurdiansyahlabs.com",
            "https://api.nurdiansyahlabs.com/",
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
    }
}
