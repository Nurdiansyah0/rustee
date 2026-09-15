# Project: Personal Finance PWA SaaS

Production-ready, full-stack Personal Finance PWA SaaS platform targeting deployment on `api.nurdiansyahlabs.com` infrastructure.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       Vue 3 Mobile-First PWA                            │
│  - Composition API (<script setup>), Vite, Tailwind CSS, Pinia          │
│  - Workbox Offline App Shell (NetworkOnly on /api/v1/*)                 │
│  - Views: Home, Transactions, Add (Rapid Keypad), Analytics, Profile    │
│  - Localized id-ID IDR formatting (Zero floating-point loss)            │
│  - FeatureLockOverlay & UpgradeModal on HTTP 403                        │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │ HTTPS / Cookies (HttpOnly; SameSite=Lax)
┌────────────────────────────────────▼────────────────────────────────────┐
│                       New Backend Architecture                          │
│  - (To be configured according to new backend stack selection)          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Core Functional Capabilities

1. **Multi-Wallet Accounts**: Support checking, savings, e-wallet, and cash balances.
2. **Category Personalization & Soft-Delete**: Income/expense categories with historical data preservation.
3. **Transaction Ledger & Net Cash Flow**: Single shared calculation engine for net cash flow (`income - expenses`).
4. **Integer Currency Handling**: Accurate minor-unit / integer Rupiah math with zero floating-point loss.
5. **Idempotent Mutations**: Unique mutation deduplication preventing double-charges / double-entries.
6. **Multi-Tenant Security**: Query-level scoping ensuring users can only access their own resources.
7. **Payment & Subscriptions**: Free vs Premium @ Rp5,000/month tier with payment gateway support (Midtrans, Xendit, DANA).
8. **Mobile-First PWA Shell**: 5-tab bottom navigation, rapid POS numeric keypad entry, and offline app caching.

## REST API Interface Contract (Frontend Expectations)

- **Base URL**: `/api/v1`
- **Authentication**: Cookie named `auth_token` with `HttpOnly; SameSite=Lax; Path=/`
- **Headers**:
  - Request: `Cache-Control: private, no-store`, `Idempotency-Key: <UUIDv4>` (on mutations)
  - Response: `Cache-Control: private, no-store, must-revalidate`
- **Error Format (RFC 7807 compatible)**:
  ```json
  {
    "type": "https://api.nurdiansyahlabs.com/errors/forbidden",
    "title": "Forbidden",
    "status": 403,
    "detail": "Subscription feature 'analytics.advanced' required.",
    "code": "FEATURE_LOCKED"
  }
  ```
