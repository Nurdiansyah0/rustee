# E2E Test Infra: Personal Finance PWA SaaS

## Test Philosophy
- Opaque-box, requirement-driven. No dependency on internal module implementation details.
- Methodology: Category-Partition + Boundary Value Analysis + Pairwise + Real-World Workload Testing.
- All tests exercise the deployed endpoints, HTTP contracts, headers, status codes, and JSON responses.

## Feature Inventory & Test Mapping

| # | Feature | Source (Requirement) | Tier 1 (Coverage ≥5) | Tier 2 (Boundary ≥5) | Tier 3 (Pairwise) | Tier 4 (Scenario) |
|---|---------|----------------------|:-------------------:|:-------------------:|:-----------------:|:-----------------:|
| 1 | SQLite WAL & PRAGMAs | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 2 | Relational Schema (10 tables) | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 3 | Composite Indexes | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 4 | Integer Rupiah Math | ORIGINAL_REQUEST §R3, AC | 5 | 5 | ✓ | ✓ |
| 5 | Multi-Tenant Data Isolation | ORIGINAL_REQUEST §R4, AC | 5 | 5 | ✓ | ✓ |
| 6 | Argon2id Password Hashing | ORIGINAL_REQUEST §R4, AC | 5 | 5 | ✓ | ✓ |
| 7 | Short-lived JWT & HttpOnly Cookies | ORIGINAL_REQUEST §R4 | 5 | 5 | ✓ | ✓ |
| 8 | Rate Limiting (5 / 15 min) | ORIGINAL_REQUEST §R4 | 5 | 5 | ✓ | ✓ |
| 9 | Pure NetCashFlowEngine | ORIGINAL_REQUEST §R1, AC | 5 | 5 | ✓ | ✓ |
| 10 | Multi-Wallet Accounts | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 11 | Category Soft-Deletion | ORIGINAL_REQUEST §R1, AC | 5 | 5 | ✓ | ✓ |
| 12 | Idempotency-Key Deduplication | ORIGINAL_REQUEST §R1, AC | 5 | 5 | ✓ | ✓ |
| 13 | Payment Gateway Trait | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 14 | HMAC/SHA-512 Verification | ORIGINAL_REQUEST §R2, AC | 5 | 5 | ✓ | ✓ |
| 15 | Idempotent Webhook Processing | ORIGINAL_REQUEST §R2, AC | 5 | 5 | ✓ | ✓ |
| 16 | Subscription Lifecycle States | ORIGINAL_REQUEST §R2, AC | 5 | 5 | ✓ | ✓ |
| 17 | Server-Side Feature Gating | ORIGINAL_REQUEST §R4, AC | 5 | 5 | ✓ | ✓ |
| 18 | Full Axum REST API | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 19 | /health & /ready Probes | ORIGINAL_REQUEST AC | 5 | 5 | ✓ | ✓ |
| 20 | Environment & Secrets Config | ORIGINAL_REQUEST AC | 5 | 5 | ✓ | ✓ |
| 21 | Vue 3 Mobile PWA Shell | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 22 | 5-Tab Mobile Navigation | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 23 | Rapid POS Numeric Keypad | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 24 | Localized id-ID IDR Formatting | ORIGINAL_REQUEST §R5, AC | 5 | 5 | ✓ | ✓ |
| 25 | Workbox Offline Caching Policy | ORIGINAL_REQUEST §R5, AC | 5 | 5 | ✓ | ✓ |
| 26 | Accessible Financial Charts | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 27 | Feature Lock Overlay & Modal | ORIGINAL_REQUEST §R4, R5 | 5 | 5 | ✓ | ✓ |
| 28 | 100% E2E Test Pass | ORIGINAL_REQUEST AC | 5 | 5 | ✓ | ✓ |
| 29 | Adversarial Coverage Hardening | Project Quality Gate | 5 | 5 | ✓ | ✓ |

## Test Architecture
- Location: `/home/nurdiansyah/teamwork_projects/personal_finance_pwa/e2e_tests`
- Runner script: `e2e_tests/runner.sh`
- Invocation: `bash e2e_tests/runner.sh [tier1|tier2|tier3|tier4|all]`
- Output format: TAP (Test Anything Protocol) / JUnit XML compatible output with zero exit code on 100% pass.

## Coverage Thresholds
- Tier 1: ≥145 test cases (≥5 per feature across 29 features)
- Tier 2: ≥145 boundary test cases (≥5 per feature across 29 features)
- Tier 3: ≥29 pairwise feature interaction tests
- Tier 4: ≥15 real-world realistic application workload scenarios
- **Total Minimum Target: ≥334 test cases**
