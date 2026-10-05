//! Empirical Adversarial Verification Test Suite for Milestone 1 (M1)
//!
//! Identity: teamwork_preview_challenger_m1_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//! Focus: WAC Integer Math, Division Rounding Boundaries, Large Numbers ($3 \times 10^{13}$ IDR)
//!
//! Invariants Challenged:
//! 1. Deterministic pure integer half-up rounding (zero truncation error / bias).
//! 2. Integer division remainder boundaries for odd and even divisors.
//! 3. Large numbers up to and exceeding $3 \times 10^{13}$ IDR (large commercial orders) without overflow.
//! 4. Intermediate cost calculations exceeding `i64::MAX` safely handled via `i128`.
//! 5. Edge cases: zero stock transition, zero incoming quantity rejection, identical unit costs.
//! 6. Free inventory dilution (Rp 0 unit costs).
//! 7. Zero floating-point math (`f32`/`f64`) across calculation paths.

use backend::domain::accounting::round_half_up_i128;
use backend::domain::inventory::calculate_weighted_average_cost;
use backend::domain::money::Rupiah;
use backend::error::AppError;

// ============================================================================
// 1. Division Truncation & Deterministic Half-Up Rounding Boundaries
// ============================================================================

#[test]
fn test_round_half_up_even_denominators_exact_halves() {
    // For even denominators d, remainder r = d / 2 is an exact half (e.g. 0.5)
    // Deterministic half-up rule requires r = d / 2 to round UP.
    // Conversely, r = (d / 2) - 1 must round DOWN.
    let even_divisors: [i128; 9] = [2, 4, 6, 8, 10, 20, 50, 100, 1000];

    for &d in &even_divisors {
        let half = d / 2;
        let base_quotient: i128 = 42;

        // Exactly at half: (base * d) + half => base + 0.5 => must round up to base + 1
        let n_half = base_quotient * d + half;
        let result_half = round_half_up_i128(n_half, d);
        assert_eq!(
            result_half,
            (base_quotient + 1) as i64,
            "Failed exact half-up for d={d}: expected {}, got {result_half}",
            base_quotient + 1
        );

        // One below half: (base * d) + half - 1 => base + <0.5 => must truncate down to base
        if half > 0 {
            let n_below = base_quotient * d + (half - 1);
            let result_below = round_half_up_i128(n_below, d);
            assert_eq!(
                result_below, base_quotient as i64,
                "Failed below-half down-truncation for d={d}: expected {base_quotient}, got {result_below}"
            );
        }

        // One above half: (base * d) + half + 1 => base + >0.5 => must round up to base + 1
        if half + 1 < d {
            let n_above = base_quotient * d + (half + 1);
            let result_above = round_half_up_i128(n_above, d);
            assert_eq!(
                result_above,
                (base_quotient + 1) as i64,
                "Failed above-half up-rounding for d={d}: expected {}, got {result_above}",
                base_quotient + 1
            );
        }
    }
}

#[test]
fn test_round_half_up_odd_denominators_boundary_split() {
    // For odd denominators d, d / 2 truncates to (d - 1) / 2.
    // The fraction ((d - 1) / 2) / d is strictly less than 0.5 => must truncate DOWN.
    // The fraction ((d + 1) / 2) / d is strictly greater than 0.5 => must round UP.
    let odd_divisors: [i128; 8] = [3, 5, 7, 9, 11, 13, 99, 999];

    for &d in &odd_divisors {
        let base_quotient: i128 = 100;
        let floor_half = d / 2; // (d - 1) / 2
        let ceil_half = floor_half + 1; // (d + 1) / 2

        // Remainder = (d - 1) / 2: strictly < 0.5 => must truncate to base
        let n_down = base_quotient * d + floor_half;
        let res_down = round_half_up_i128(n_down, d);
        assert_eq!(
            res_down, base_quotient as i64,
            "Failed odd denominator down-truncation for d={d}: expected {base_quotient}, got {res_down}"
        );

        // Remainder = (d + 1) / 2: strictly > 0.5 => must round up to base + 1
        let n_up = base_quotient * d + ceil_half;
        let res_up = round_half_up_i128(n_up, d);
        assert_eq!(
            res_up,
            (base_quotient + 1) as i64,
            "Failed odd denominator up-rounding for d={d}: expected {}, got {res_up}",
            base_quotient + 1
        );
    }
}

#[test]
fn test_round_half_up_zero_and_edge_denominators() {
    // Zero numerator
    assert_eq!(round_half_up_i128(0, 1), 0);
    assert_eq!(round_half_up_i128(0, 100), 0);
    assert_eq!(round_half_up_i128(0, 1_000_000), 0);

    // Divisor = 0 should safely return 0 without division by zero panic
    assert_eq!(round_half_up_i128(100, 0), 0);
    assert_eq!(round_half_up_i128(0, 0), 0);

    // Exact division: remainder 0
    assert_eq!(round_half_up_i128(100, 10), 10);
    assert_eq!(round_half_up_i128(300, 3), 100);
    assert_eq!(round_half_up_i128(1_000_000_000, 1), 1_000_000_000);
}

#[test]
fn test_round_half_up_deterministic_oracle_sweep() {
    // Oracle function using 2 * remainder >= denominator comparison
    fn oracle_round_half_up(n: i128, d: i128) -> i64 {
        if d == 0 {
            return 0;
        }
        let q = n / d;
        let r = n % d;
        if 2 * r >= d {
            (q + 1) as i64
        } else {
            q as i64
        }
    }

    // Pseudorandom deterministic LCG generator
    let mut state: u64 = 0xDEADBEEFCAFE;
    let mut next_u64 = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        state
    };

    // Stress 20,000 diverse combinations of numerator and denominator
    for _ in 0..20_000 {
        let d = (next_u64() % 10_000 + 1) as i128; // 1 <= d <= 10,000
        let n = (next_u64() % 1_000_000_000) as i128;

        let expected = oracle_round_half_up(n, d);
        let actual = round_half_up_i128(n, d);
        assert_eq!(
            actual, expected,
            "Oracle mismatch for n={n}, d={d}: expected {expected}, got {actual}"
        );
    }
}

// ============================================================================
// 2. Commercial Scale & Large Numbers Stress Testing ($3 \times 10^{13}$ IDR)
// ============================================================================

#[test]
fn test_wac_large_numbers_commercial_orders_30_trillion_idr() {
    // Large commercial order: unit costs around Rp 30.000.000.000.000 (30 trillion IDR)
    let thirty_trillion = Rupiah::new(30_000_000_000_000);
    let fifteen_trillion = Rupiah::new(15_000_000_000_000);
    let twenty_trillion = Rupiah::new(20_000_000_000_000);
    let twenty_five_trillion = Rupiah::new(25_000_000_000_000);

    // 1 unit @ 30T + 2 units @ 15T = 3 units @ 20T
    let res1 = calculate_weighted_average_cost(1, thirty_trillion, 2, fifteen_trillion).unwrap();
    assert_eq!(res1, twenty_trillion);

    // 10,000 units @ 30T + 5,000 units @ 15T = 15,000 units @ 25T
    let res2 = calculate_weighted_average_cost(10_000, thirty_trillion, 5_000, fifteen_trillion).unwrap();
    assert_eq!(res2, twenty_five_trillion);

    // Rounding check with large numbers:
    // 1 unit @ Rp 30.000.000.000.000 + 1 unit @ Rp 30.000.000.000.001
    // Total cost = 60.000.000.000.001, total qty = 2 => 30.000.000.000.000,5
    // Must round half-up to Rp 30.000.000.000.001
    let res3 = calculate_weighted_average_cost(
        1,
        thirty_trillion,
        1,
        Rupiah::new(30_000_000_000_001),
    )
    .unwrap();
    assert_eq!(res3, Rupiah::new(30_000_000_000_001));

    // And opposite order:
    let res4 = calculate_weighted_average_cost(
        1,
        Rupiah::new(30_000_000_000_001),
        1,
        thirty_trillion,
    )
    .unwrap();
    assert_eq!(res4, Rupiah::new(30_000_000_000_001));
}

#[test]
fn test_wac_intermediate_cost_exceeding_i64_max() {
    // Intermediate cost exceeding i64::MAX (9.22 x 10^18 IDR)
    // 1,000,000 units @ Rp 30.000.000.000.000 = 30 x 10^18 IDR (exceeds i64::MAX!)
    // 1,000,000 units @ Rp 10.000.000.000.000 = 10 x 10^18 IDR
    // Total intermediate cost = 40 x 10^18 IDR.
    // In i128, this fits effortlessly without overflow.
    let prev_qty = 1_000_000;
    let prev_cost = Rupiah::new(30_000_000_000_000);
    let in_qty = 1_000_000;
    let in_cost = Rupiah::new(10_000_000_000_000);

    let res = calculate_weighted_average_cost(prev_qty, prev_cost, in_qty, in_cost).unwrap();
    assert_eq!(res, Rupiah::new(20_000_000_000_000));
}

#[test]
fn test_wac_extreme_quadrillion_and_theoretical_max_bounds() {
    // Quadrillion scale quantities with thirty trillion IDR unit costs:
    // 10^9 units @ 30T + 10^9 units @ 10T => Total cost = 4 x 10^22 IDR.
    let res_billion_qty = calculate_weighted_average_cost(
        1_000_000_000,
        Rupiah::new(30_000_000_000_000),
        1_000_000_000,
        Rupiah::new(10_000_000_000_000),
    )
    .unwrap();
    assert_eq!(res_billion_qty, Rupiah::new(20_000_000_000_000));

    // 10^12 units @ 30T + 10^12 units @ 10T => Total cost = 4 x 10^25 IDR.
    let res_trillion_qty = calculate_weighted_average_cost(
        1_000_000_000_000,
        Rupiah::new(30_000_000_000_000),
        1_000_000_000_000,
        Rupiah::new(10_000_000_000_000),
    )
    .unwrap();
    assert_eq!(res_trillion_qty, Rupiah::new(20_000_000_000_000));

    // Absolute theoretical maximum i64 boundary:
    // Both quantities = i64::MAX, both unit costs = Rupiah::MAX (i64::MAX)
    // Product 1: (2^63 - 1)^2
    // Product 2: (2^63 - 1)^2
    // Sum: 2 * (2^63 - 1)^2 = 2^127 - 2^65 + 2 < 2^127 - 1 (i128::MAX).
    // Zero panic, zero overflow.
    let res_max = calculate_weighted_average_cost(
        i64::MAX,
        Rupiah::MAX,
        i64::MAX,
        Rupiah::MAX,
    )
    .unwrap();
    assert_eq!(res_max, Rupiah::MAX);

    // Asymmetric weight stress test:
    // 1 unit @ Rp 30.000.000.000.000 + 1.000.000.000 units @ Rp 1
    // Total cost = 30.000.000.000.000 + 1.000.000.000 = 30.001.000.000.000
    // Total qty = 1.000.000.001
    // Division: 30.001.000.000.000 / 1.000.000.001 = 30.000,999969... => rounds half-up to 30.001 IDR
    let res_asymmetric = calculate_weighted_average_cost(
        1,
        Rupiah::new(30_000_000_000_000),
        1_000_000_000,
        Rupiah::new(1),
    )
    .unwrap();
    assert_eq!(res_asymmetric, Rupiah::new(30_001));
}

// ============================================================================
// 3. Boundary & Edge Cases (Zero Stock, Zero Receipt, Identical Costs)
// ============================================================================

#[test]
fn test_wac_zero_stock_transition_edge_case() {
    // When previous stock is 0, incoming unit cost must completely become the new average cost,
    // regardless of what previous average cost was recorded.
    let test_cases = [
        (0, 10_000),
        (50_000, 12_500),
        (30_000_000_000_000, 1_000_000),
        (i64::MAX, 25_000),
    ];

    for &(old_cost, new_cost) in &test_cases {
        let wac = calculate_weighted_average_cost(
            0,
            Rupiah::new(old_cost),
            100,
            Rupiah::new(new_cost),
        )
        .unwrap();
        assert_eq!(
            wac,
            Rupiah::new(new_cost),
            "Zero initial stock must take new unit cost exactly. old_cost={old_cost}, new_cost={new_cost}"
        );
    }
}

#[test]
fn test_wac_zero_received_qty_and_negative_quantities_rejected() {
    // Receiving 0 units must be rejected with INVALID_QUANTITY
    let err_zero_in = calculate_weighted_average_cost(
        10,
        Rupiah::new(10_000),
        0,
        Rupiah::new(10_000),
    );
    match err_zero_in {
        Err(AppError::BadRequest(_, code)) => assert_eq!(code, "INVALID_QUANTITY"),
        other => panic!("Expected BadRequest INVALID_QUANTITY, got: {:?}", other),
    }

    // Negative incoming quantity must be rejected
    let err_neg_in = calculate_weighted_average_cost(
        10,
        Rupiah::new(10_000),
        -5,
        Rupiah::new(10_000),
    );
    match err_neg_in {
        Err(AppError::BadRequest(_, code)) => assert_eq!(code, "INVALID_QUANTITY"),
        other => panic!("Expected BadRequest INVALID_QUANTITY, got: {:?}", other),
    }

    // Negative previous quantity must be rejected
    let err_neg_prev = calculate_weighted_average_cost(
        -1,
        Rupiah::new(10_000),
        10,
        Rupiah::new(10_000),
    );
    match err_neg_prev {
        Err(AppError::BadRequest(_, code)) => assert_eq!(code, "INVALID_QUANTITY"),
        other => panic!("Expected BadRequest INVALID_QUANTITY, got: {:?}", other),
    }
}

#[test]
fn test_wac_identical_unit_costs_invariant() {
    // When previous cost and incoming cost are identical (C),
    // the resulting WAC must identically equal C for ANY quantity ratio.
    let costs: [i64; 7] = [
        0,
        1,
        100,
        15_000,
        123_456_789,
        30_000_000_000_000,
        i64::MAX,
    ];
    let qty_pairs: [(i64, i64); 6] = [
        (1, 1),
        (1, 2),
        (100, 1),
        (1, 1_000_000),
        (500_000, 500_000),
        (1_000_000_000, 2_000_000_000),
    ];

    for &cost in &costs {
        for &(q1, q2) in &qty_pairs {
            let res = calculate_weighted_average_cost(
                q1,
                Rupiah::new(cost),
                q2,
                Rupiah::new(cost),
            )
            .unwrap();
            assert_eq!(
                res,
                Rupiah::new(cost),
                "Identical cost invariant failed for cost={cost}, q1={q1}, q2={q2}"
            );
        }
    }
}

#[test]
fn test_wac_free_inventory_dilution_and_zero_cost_edges() {
    // Inbound free items (Rp 0 cost price, e.g. promo bonus / gift):
    // 10 units @ Rp 10.000 + 10 units @ Rp 0 => 20 units @ Rp 5.000
    let res1 = calculate_weighted_average_cost(10, Rupiah::new(10_000), 10, Rupiah::ZERO).unwrap();
    assert_eq!(res1, Rupiah::new(5_000));

    // 10 units @ Rp 10.000 + 30 units @ Rp 0 => 40 units @ Rp 2.500
    let res2 = calculate_weighted_average_cost(10, Rupiah::new(10_000), 30, Rupiah::ZERO).unwrap();
    assert_eq!(res2, Rupiah::new(2_500));

    // 10 units @ Rp 10.000 + 1 unit @ Rp 0 => 11 units @ Rp 100.000 / 11 = 9.090,909... => 9.091 IDR
    let res3 = calculate_weighted_average_cost(10, Rupiah::new(10_000), 1, Rupiah::ZERO).unwrap();
    assert_eq!(res3, Rupiah::new(9_091));

    // Existing zero cost stock diluted with new priced receipt:
    // 10 units @ Rp 0 + 10 units @ Rp 10.000 => 20 units @ Rp 5.000
    let res4 = calculate_weighted_average_cost(10, Rupiah::ZERO, 10, Rupiah::new(10_000)).unwrap();
    assert_eq!(res4, Rupiah::new(5_000));

    // Both zero cost:
    let res5 = calculate_weighted_average_cost(100, Rupiah::ZERO, 50, Rupiah::ZERO).unwrap();
    assert_eq!(res5, Rupiah::ZERO);
}

// ============================================================================
// 4. Memory Layout, Type Representation & Zero Floating-Point Invariants
// ============================================================================

#[test]
fn test_rupiah_type_representation_and_zero_float() {
    // Rupiah must be an exact 64-bit integer value object
    assert_eq!(std::mem::size_of::<Rupiah>(), 8);
    assert_eq!(std::mem::align_of::<Rupiah>(), 8);

    // Verify transparent serde serialization formats as native integer, NOT float
    let r = Rupiah::new(15_000);
    let serialized = serde_json::to_string(&r).unwrap();
    assert_eq!(serialized, "15000");

    let deserialized: Rupiah = serde_json::from_str("15000").unwrap();
    assert_eq!(deserialized, r);

    // Deserialization of float string or non-integer is handled according to serde integer rules
    let err_deser = serde_json::from_str::<Rupiah>("15000.50");
    assert!(
        err_deser.is_err(),
        "Rupiah must not deserialize from floating point JSON"
    );
}

// ============================================================================
// 5. Extended Mathematical Probes & Sequential Receipts
// ============================================================================

#[test]
fn test_wac_prime_denominators_fractional_split() {
    // Test prime denominators across different fractions
    let primes: [i64; 6] = [7, 11, 13, 17, 19, 23];

    for &p in &primes {
        // Construct quantities: prev_qty = 1, in_qty = p - 1 => total_qty = p
        // Case A: remainder is floor(p / 2), fraction < 0.5 => must truncate DOWN
        // prev_cost = 0, in_cost = C
        // total_cost = (p - 1) * C. We want total_cost % p == floor(p / 2).
        let floor_half = p / 2;
        // round_half_up(floor_half, p) => 0
        assert_eq!(
            round_half_up_i128(floor_half as i128, p as i128),
            0,
            "Prime p={p}: floor_half={floor_half} must truncate to 0"
        );

        // Case B: remainder is floor(p / 2) + 1, fraction > 0.5 => must round UP
        let ceil_half = floor_half + 1;
        assert_eq!(
            round_half_up_i128(ceil_half as i128, p as i128),
            1,
            "Prime p={p}: ceil_half={ceil_half} must round up to 1"
        );
    }
}

#[test]
fn test_wac_sequential_receipts_accumulation() {
    // Realistic multi-step inventory receipt workflow:
    // Step 0: Initial state: 0 units @ Rp 0
    let mut current_qty: i64 = 0;
    let mut current_wac = Rupiah::ZERO;

    // Receipt 1: Inbound 100 units @ Rp 10.000
    current_wac = calculate_weighted_average_cost(
        current_qty,
        current_wac,
        100,
        Rupiah::new(10_000),
    )
    .unwrap();
    current_qty += 100;
    assert_eq!(current_qty, 100);
    assert_eq!(current_wac, Rupiah::new(10_000));

    // Receipt 2: Inbound 50 units @ Rp 12.000
    // Total cost: 100 * 10.000 + 50 * 12.000 = 1.000.000 + 600.000 = 1.600.000
    // Total qty: 150
    // 1.600.000 / 150 = 10.666,666... => rounds half-up to 10.667 IDR
    current_wac = calculate_weighted_average_cost(
        current_qty,
        current_wac,
        50,
        Rupiah::new(12_000),
    )
    .unwrap();
    current_qty += 50;
    assert_eq!(current_qty, 150);
    assert_eq!(current_wac, Rupiah::new(10_667));

    // Receipt 3: Inbound 100 units @ Rp 9.500
    // Total cost: 150 * 10.667 + 100 * 9.500 = 1.600.050 + 950.000 = 2.550.050
    // Total qty: 250
    // 2.550.050 / 250 = 10.200,2 => rounds down to 10.200 IDR
    current_wac = calculate_weighted_average_cost(
        current_qty,
        current_wac,
        100,
        Rupiah::new(9_500),
    )
    .unwrap();
    current_qty += 100;
    assert_eq!(current_qty, 250);
    assert_eq!(current_wac, Rupiah::new(10_200));

    // Outbound mutation: selling 200 units
    // Outbound movements preserve the existing WAC (cost of goods sold is 200 * 10.200)
    current_qty -= 200;
    assert_eq!(current_qty, 50);
    assert_eq!(current_wac, Rupiah::new(10_200));

    // Receipt 4: Inbound 50 units @ Rp 11.000
    // Total cost: 50 * 10.200 + 50 * 11.000 = 510.000 + 550.000 = 1.060.000
    // Total qty: 100
    // 1.060.000 / 100 = 10.600 IDR exactly
    current_wac = calculate_weighted_average_cost(
        current_qty,
        current_wac,
        50,
        Rupiah::new(11_000),
    )
    .unwrap();
    current_qty += 50;
    assert_eq!(current_qty, 100);
    assert_eq!(current_wac, Rupiah::new(10_600));
}

#[test]
fn test_wac_negative_unit_cost_adversarial_probe() {
    // Adversarial finding probe:
    // calculate_weighted_average_cost does not currently validate whether unit costs are negative.
    // If negative unit cost is passed, round_half_up_i128((n + d/2)/d) shifts the negative numerator
    // towards zero (+5 on -50,000 gives -49,995 / 10 = -4,999 instead of -5,000).
    let probe = calculate_weighted_average_cost(
        0,
        Rupiah::ZERO,
        10,
        Rupiah::new(-5_000),
    );
    // Empirical observation: function succeeds returning Rupiah(-4999).
    // Note: Negative costs are strictly rejected at the database persistence layer
    // via CHECK (unit_cost >= 0) and CHECK (cost_price >= 0).
    // Recommendation for M3: calculate_weighted_average_cost should defensively reject negative costs.
    assert!(probe.is_ok());
    assert_eq!(probe.unwrap(), Rupiah::new(-4_999));
}

// ============================================================================
// 6. Database Schema Layer Check Constraint Verification
// ============================================================================

#[tokio::test]
async fn test_db_schema_strictly_rejects_negative_costs_and_stock() {
    use backend::repository::{init_pool, run_migrations, DbConfig};
    use tempfile::tempdir;

    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_db_wac.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 5,
        min_connections: 1,
        busy_timeout_ms: 5000,
        acquire_timeout_secs: 5,
    };

    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool).await.expect("Failed to run migrations");

    let tenant_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Seed tenant
    sqlx::query(
        "INSERT INTO tenants (id, name, slug, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&tenant_id)
    .bind("WAC Test Tenant")
    .bind("wac-test")
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("Failed to insert tenant");

    // 2. Attempt to insert product with negative cost_price -> MUST FAIL
    let prod_err = sqlx::query(
        "INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&tenant_id)
    .bind("SKU-NEG-COST")
    .bind("Negative Cost Product")
    .bind("PCS")
    .bind(-1000) // INVALID: negative cost_price
    .bind(15000)
    .bind(10)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await;
    assert!(
        prod_err.is_err(),
        "Database must strictly reject negative cost_price via CHECK constraint"
    );

    // 3. Attempt to insert warehouse
    let wh_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
         VALUES (?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&wh_id)
    .bind(&tenant_id)
    .bind("WH-01")
    .bind("Main Warehouse")
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("Failed to insert warehouse");

    // 4. Valid product
    let prod_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 10000, 15000, 10, 1, ?, ?)",
    )
    .bind(&prod_id)
    .bind(&tenant_id)
    .bind("SKU-VALID")
    .bind("Valid Product")
    .bind("PCS")
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("Failed to insert valid product");

    // 5. Attempt to insert stock_item with negative average_cost -> MUST FAIL
    let stock_cost_err = sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, average_cost, updated_at)
         VALUES (?, ?, ?, ?, 100, 0, 10, -500, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .bind(&now)
    .execute(&pool)
    .await;
    assert!(
        stock_cost_err.is_err(),
        "Database must strictly reject negative average_cost in stock_items via CHECK constraint"
    );

    // 6. Attempt to insert stock_movement with negative unit_cost -> MUST FAIL
    let sm_err = sqlx::query(
        "INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, destination_warehouse_id, quantity, unit_cost, created_at)
         VALUES (?, ?, 'INBOUND', ?, ?, 10, -100, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&tenant_id)
    .bind(&prod_id)
    .bind(&wh_id)
    .bind(&now)
    .execute(&pool)
    .await;
    assert!(
        sm_err.is_err(),
        "Database must strictly reject negative unit_cost in stock_movements via CHECK constraint"
    );
}

