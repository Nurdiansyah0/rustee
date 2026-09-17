package com.invinite.pwa

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Unit tests for BackgroundSyncWorker constants and configuration.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 25 & § 27
 */
class BackgroundSyncWorkerTest {

    @Test
    fun testWorkManagerConstants() {
        assertEquals("invinite_periodic_sync", BackgroundSyncWorker.PERIODIC_WORK_NAME)
        assertEquals("invinite_one_time_sync", BackgroundSyncWorker.ONE_TIME_WORK_NAME)
    }
}
