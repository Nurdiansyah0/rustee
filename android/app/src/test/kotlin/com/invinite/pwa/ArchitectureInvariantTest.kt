package com.invinite.pwa

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Architectural Invariant Test verifying that the native Android layer
 * strictly adheres to the Native Layer Invariant defined in Master Specification v3.1.0:
 *
 * "The native layer must NEVER own financial calculations, subscription authority,
 * or ledger persistence. All financial logic belongs exclusively to the backend."
 */
class ArchitectureInvariantTest {

    private val forbiddenFinancialCalculationTerms = listOf(
        "compute_net_cash_flow",
        "calculate_balance",
        "calculate_interest",
        "double_entry_ledger",
        "CREATE TABLE transactions",
        "CREATE TABLE accounts",
        "CREATE TABLE ledger",
        "Room.databaseBuilder",
        "grantPremiumAccess",
        "override_subscription_tier",
        "bypass_feature_gate"
    )

    @Test
    fun testNativeLayerContainsNoFinancialCalculationOrAuthorityLogic() {
        val srcDir = File("src/main/kotlin/com/invinite/pwa")
        assertTrue("Source directory must exist", srcDir.exists())

        val kotlinFiles = srcDir.walkTopDown().filter { it.extension == "kt" }.toList()
        assertTrue("Must have Kotlin source files in com.invinite.pwa", kotlinFiles.isNotEmpty())

        for (file in kotlinFiles) {
            val content = file.readText()

            for (forbidden in forbiddenFinancialCalculationTerms) {
                assertFalse(
                    "Violation of Native Layer Invariant: File ${file.name} contains forbidden term '$forbidden'. All financial calculations and subscription authority belong exclusively to the backend.",
                    content.contains(forbidden, ignoreCase = true)
                )
            }
        }
    }

    @Test
    fun testRequiredAndroidComponentsPresent() {
        val srcDir = File("src/main/kotlin/com/invinite/pwa")
        val requiredFiles = listOf(
            "MainActivity.kt",
            "InviniteBridge.kt",
            "NotificationListener.kt",
            "BackgroundSyncWorker.kt",
            "SecureStorage.kt"
        )

        for (fileName in requiredFiles) {
            val file = File(srcDir, fileName)
            assertTrue("Required component $fileName must exist", file.exists())
            assertTrue("Component $fileName must not be empty", file.length() > 0)
        }
    }
}
