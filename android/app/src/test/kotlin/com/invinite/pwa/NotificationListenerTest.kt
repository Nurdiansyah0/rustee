package com.invinite.pwa

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for NotificationListener banking/fintech filtering,
 * canonical payload extraction, and data minimization.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 21 & § 24
 * - REQ-INGEST-05: Target package filtering
 * - REQ-INGEST-08: Payload minimization
 */
class NotificationListenerTest {

    @Test
    fun testTargetBankingAndFintechPackagesAccepted() {
        val targets = listOf(
            "com.bca",
            "com.bca.mobile",
            "com.bca.mybca",
            "com.bankmandiri.livin",
            "com.bankmandiri.mandirionline",
            "id.co.bri.brimo",
            "id.co.bni.mbanking",
            "id.dana",
            "com.gojek.app",
            "com.gopay.wallet",
            "ovo.id",
            "com.shopee.id"
        )

        for (pkg in targets) {
            assertTrue("Expected target package $pkg to be accepted", NotificationListener.isTargetPackage(pkg))
        }
    }

    @Test
    fun testNonFinancialPackagesRejected() {
        val nonTargets = listOf(
            "com.whatsapp",
            "com.google.android.youtube",
            "com.facebook.katana",
            "com.instagram.android",
            "com.twitter.android",
            "com.spotify.music",
            "com.bca.spoofed.attacker",
            "id.dana.fake.malware",
            "",
            null
        )

        for (pkg in nonTargets) {
            assertFalse("Expected non-target package $pkg to be rejected", NotificationListener.isTargetPackage(pkg))
        }
    }

    @Test
    fun testExtractCanonicalNotificationSuccess() {
        val canonical = NotificationListener.extractCanonical(
            packageName = "com.bca.mobile",
            title = "m-Transfer Berhasil",
            text = "Transfer Rp 50.000 ke BCA 1234567890 BERHASIL",
            bigText = null,
            timestamp = 1773715200000L
        )

        assertNotNull("Canonical notification must not be null for target package", canonical)
        assertEquals("com.bca.mobile", canonical?.packageName)
        assertEquals("m-Transfer Berhasil", canonical?.title)
        assertEquals("Transfer Rp 50.000 ke BCA 1234567890 BERHASIL", canonical?.text)
        assertEquals(1773715200000L, canonical?.postedAt)
    }

    @Test
    fun testBigTextPreferredOverStandardText() {
        val canonical = NotificationListener.extractCanonical(
            packageName = "id.dana",
            title = "Pembayaran Berhasil",
            text = "Pembayaran Rp 25.000",
            bigText = "Pembayaran Rp 25.000 ke Merchant Kopi Kenangan telah berhasil diselesaikan.",
            timestamp = 1773715300000L
        )

        assertNotNull(canonical)
        assertEquals(
            "Pembayaran Rp 25.000 ke Merchant Kopi Kenangan telah berhasil diselesaikan.",
            canonical?.text
        )
    }

    @Test
    fun testNonTargetPackageReturnsNull() {
        val canonical = NotificationListener.extractCanonical(
            packageName = "com.whatsapp",
            title = "Pesan baru dari Budi",
            text = "Halo apa kabar?",
            bigText = null,
            timestamp = 1773715400000L
        )

        assertNull("Non-target package must be rejected and return null", canonical)
    }

    @Test
    fun testEmptyTitleAndTextReturnsNull() {
        val canonical = NotificationListener.extractCanonical(
            packageName = "com.bca",
            title = "   ",
            text = "",
            bigText = null,
            timestamp = 1773715500000L
        )

        assertNull("Empty content notifications must be discarded", canonical)
    }

    @Test
    fun testCanonicalJsonRepresentationSchema() {
        val canonical = NotificationListener.Companion.CanonicalNotification(
            packageName = "com.gojek.app",
            title = "GoPay: Transaksi Berhasil",
            text = "Kamu telah membayar Rp 35.000 di Alfamart",
            postedAt = 1773715600000L
        )

        val jsonString = canonical.toJsonString()
        val json = JSONObject(jsonString)

        assertEquals("com.gojek.app", json.getString("package_name"))
        assertEquals("GoPay: Transaksi Berhasil", json.getString("title"))
        assertEquals("Kamu telah membayar Rp 35.000 di Alfamart", json.getString("text"))
        assertEquals(1773715600000L, json.getLong("posted_at"))

        // Verification of REQ-INGEST-08: Payload minimization (no raw extras/PII fields)
        assertEquals(4, json.length())
        assertFalse(json.has("raw_extras"))
        assertFalse(json.has("imei"))
        assertFalse(json.has("phone_number"))
    }
}
