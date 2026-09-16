**# Original User Request**

**## 2026-09-12T10:25:22Z**

Build a production-ready, full-stack Personal Finance PWA SaaS platform targeting deployment on the \`api.nurdiansyahlabs.com\` infrastructure, powered by a consolidated 100% Rust backend (Axum / Tokio / SQLx) owning an embedded SQLite database in WAL mode, coupled with a reactive Vue 3 + Vite + Tailwind CSS mobile-first Progressive Web Application.

Working directory: /home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa

Integrity mode: development

**## Requirements**

**### R1. Consolidated Rust Backend & Layered Domain Architecture**

Deliver a production-grade backend in Rust using Axum and Tokio implementing the layered architecture (API → Service → Domain → Repository → DB). The service owns all domain logic: multi-wallet accounts, custom category personalization (income/expense with soft-deletion), and transaction recording with strict \`Idempotency-Key\` deduplication, atomic balance maintenance, and a single shared calculation engine for net cash flow (\`income - expenses\`).

**### R2. Native Rust Payment & Subscription Webhook Engine**

Implement the subscription management (Free vs Premium @ Rp5,000/month) and payment gateway integration directly within the Rust service (abstracting providers such as Midtrans/Xendit). The webhook endpoint must verify cryptographic provider signatures (HMAC) and process events idempotently using unique event IDs to prevent duplicate activations or renewals.

**### R3. Exclusive SQLite Relational Persistence (WAL Mode)**

Design and execute migrations for an embedded SQLite database (\`users\`, \`accounts\`, \`categories\`, \`transactions\`, \`budgets\`, \`goals\`, \`subscriptions\`, \`audit\_logs\`). Configure SQLite with \`PRAGMA journal\_mode = WAL\`, \`PRAGMA busy\_timeout = 5000\`, and foreign key enforcement. Store all monetary amounts strictly as integer minor units / integer Rupiah with UTC timestamps and composite indexes on \`(user\_id, date)\` and \`(account\_id)\`.

**### R4. Multi-Tenant Auth, Security & Server-Side Feature Gating**

Implement authentication using short-lived tokens and secure httpOnly, SameSite cookies with rate limiting (5 attempts / 15 min) and bcrypt/argon2 password hashing. Enforce multi-tenant data isolation (\`WHERE user\_id = \:auth\_user\_id\`) across all repositories. Enforce server-side feature gating via permission strings (\`transactions.basic\`, \`analytics.advanced\`, \`budgeting\`, \`reports.advanced\`) where locked Premium features remain visible with upgrade CTAs in the UI but strictly return HTTP 403 upon unauthorized API access.

**### R5. Mobile-First Vue 3 PWA Frontend (Vite + Tailwind CSS + Workbox)**

Deliver a modern, accessible, installable Progressive Web Application built with Vue 3 (Composition API), Vite, Tailwind CSS, and Workbox. Provide an offline app shell, responsive mobile navigation (Home, Transactions, Add, Analytics, Profile), rapid numeric keypad entry, accessible charts, and localized currency formatting (\`id-ID\`, IDR). Financial data endpoints must declare \`Cache-Control: private, no-store\`.

**## Acceptance Criteria**

**### Financial Calculations & Ledger Integrity**

\- [ ] Financial calculations (account balance, net cash flow, category aggregations) are verified by deterministic automated unit tests with fixed fixtures.

\- [ ] Monetary amounts are stored and computed strictly using integer Rupiah / integer math with zero floating-point precision loss.

\- [ ] Submitting transactions with the same \`Idempotency-Key\` concurrently or sequentially returns the original transaction without creating duplicates.

\- [ ] Deleting categories archives/soft-deletes them, preserving historical transaction integrity.

**### Security, Auth & Data Isolation**

\- [ ] Multi-tenant isolation verified: Cross-user resource access by ID is rejected with HTTP 404 or 403.

\- [ ] Password hashing uses bcrypt or argon2; authentication authorization decisions are strictly server-side.

\- [ ] Server-side feature gating verified: Unsubscribed users attempting to access Premium endpoints receive HTTP 403.

\- [ ] Zero secrets, API keys, or payment credentials in client bundles or Git tracking; \`.env.example\` provided.

**### Payment & Webhook Resilience**

\- [ ] Payment webhook handler validates cryptographic provider signatures and rejects invalid or tampered signatures.

\- [ ] Replayed payment webhooks with identical event IDs are processed idempotently without multiple subscription activations.

\- [ ] Subscription status changes (Active, Grace, Cancelled, Expired) immediately adjust user feature permissions.

**### PWA & User Experience**

\- [ ] PWA service worker installs cleanly and serves the app shell offline.

\- [ ] Vue 3 mobile layout is responsive, thumb-friendly, and accessible across mobile screen sizes.

\- [ ] Liveness (\`/health\`) and readiness (\`/ready\`) endpoints return operational status with zero secret leaks.

**## 2026-09-12T16:33:05Z**

/teamwork-preview

Use the full UI/UX, product design, frontend, backend, architecture, and QA team.

CRITICAL PRIORITY:

The current Personal Finance PWA functionality is substantially complete, but the UI/UX quality is NOT acceptable for a production SaaS product.

The immediate priority is NOT monorepo migration.

The immediate priority is to transform the Personal Finance PWA from a technically functional application into a polished, premium, modern, trustworthy personal-finance SaaS product.

The current UI should be treated as a starting point, not as a design constraint.

Do NOT preserve poor UI decisions simply because they already exist in the codebase.

**==================================================**

PRIORITY ORDER

**==============**

Priority 1 — Product UX / UI redesign

Priority 2 — Frontend implementation quality

Priority 3 — UX consistency and responsive behavior

Priority 4 — Accessibility and interaction quality

Priority 5 — Visual identity / design system

Priority 6 — Regression testing

Priority 7 — Monorepo architecture

Do NOT let monorepo restructuring distract from or delay the UI/UX transformation.

**==================================================**

PHASE 1 — UI/UX AUDIT

**=====================**

Perform a complete UX/UI audit of the existing Personal Finance PWA.

Inspect every user-facing screen and flow, including:

\* Landing/login

\* Registration

\* Authentication

\* Dashboard

\* Accounts / wallets

\* Transactions

\* Add transaction flow

\* Categories

\* Analytics

\* Budgeting

\* Reports

\* Goals

\* Subscription / upgrade

\* Profile

\* Settings

\* Empty states

\* Loading states

\* Error states

\* Premium locked states

\* Mobile navigation

\* PWA behavior

Evaluate:

\* visual hierarchy

\* information architecture

\* typography

\* spacing

\* grid system

\* component consistency

\* color system

\* iconography

\* charts

\* data visualization

\* navigation

\* forms

\* input ergonomics

\* button hierarchy

\* responsive behavior

\* mobile usability

\* accessibility

\* interaction feedback

\* animation

\* loading experience

\* empty states

\* error handling

\* perceived performance

\* trust signals

\* fintech visual credibility

Identify exactly why the current UI feels poor, amateur, generic, cluttered, or inconsistent.

Do not give generic design advice.

Tie every finding to an actual screen/component in the existing project.

**==================================================**

PHASE 2 — PRODUCT DESIGN DIRECTION

**==================================**

Redesign the product experience around this principle:

"Personal finance should feel clear, trustworthy, intelligent, and effortless — not like an accounting spreadsheet."

The UI should feel like a serious modern fintech SaaS product.

Reference the interaction quality and information hierarchy of products such as:

\* Mercury

\* Ramp

\* Brex

\* Linear

\* Stripe

\* modern premium fintech dashboards

Do NOT copy their branding.

Study their design principles:

\* strong information hierarchy

\* restrained visual language

\* clear primary metrics

\* progressive disclosure

\* contextual actions

\* calm layouts

\* meaningful data visualization

\* excellent typography

\* purposeful whitespace

\* strong responsive behavior

Avoid:

\* generic Bootstrap-style dashboards

\* excessive cards

\* excessive rounded containers

\* colorful icon grids

\* unnecessary gradients

\* excessive shadows

\* decorative charts

\* dashboard clutter

\* random colors for every metric

\* "AI-generated SaaS template" aesthetics

\* excessive glassmorphism

\* excessive animation

The interface must look intentionally designed.

**==================================================**

PHASE 3 — DESIGN SYSTEM

**=======================**

Create a proper design system before implementing individual pages.

Define:

\* typography scale

\* font hierarchy

\* spacing scale

\* layout grid

\* border radius system

\* elevation system

\* semantic colors

\* surface colors

\* text colors

\* success/warning/error colors

\* chart colors

\* interactive states

\* focus states

\* disabled states

\* dark/light theme strategy

\* icon sizing

\* button hierarchy

\* input hierarchy

Use semantic design tokens.

Do NOT scatter hardcoded colors and spacing values throughout components.

The design system must support future rebranding.

**==================================================**

PHASE 4 — INFORMATION ARCHITECTURE

**==================================**

Redesign navigation and page hierarchy.

The user should understand within seconds:

1\. How much money do I have?

2\. Where did my money go?

3\. What changed recently?

4\. What needs my attention?

5\. What should I do next?

Dashboard should NOT become a wall of charts.

Establish a clear hierarchy:

Tier 1:

Financial pulse / primary balance / important KPI

Tier 2:

Cash flow and spending context

Tier 3:

Recent transactions and actionable information

Tier 4:

Detailed analytics available through drill-down

Use progressive disclosure.

Do not expose every feature simultaneously.

**==================================================**

PHASE 5 — DASHBOARD REDESIGN

**============================**

Completely redesign the main dashboard.

The dashboard should communicate:

\* total available balance

\* income

\* expenses

\* net cash flow

\* spending trend

\* recent transactions

\* account/wallet status

\* budget progress

\* financial alerts

\* useful next actions

Include contextual comparison where meaningful:

\* this month vs previous month

\* income vs expenses

\* spending trend

\* budget remaining

Do NOT use charts simply because charts are available.

Every visualization must answer a user question.

**==================================================**

PHASE 6 — TRANSACTION UX

**========================**

The transaction experience is one of the most important parts of the application.

Redesign:

\* transaction list

\* filters

\* search

\* transaction creation

\* editing

\* category selection

\* account selection

\* amount input

\* date selection

\* notes

\* validation

\* success feedback

\* error handling

The "Add Transaction" experience must be extremely fast on mobile.

Minimize cognitive load.

Optimize for thumb interaction.

Preserve idempotency behavior and backend correctness.

Do NOT sacrifice backend safety for UI convenience.

**==================================================**

PHASE 7 — ANALYTICS UX

**======================**

Redesign analytics around decisions rather than charts.

Instead of:

"Here are 8 charts."

Use:

"What happened?"

"Why did it happen?"

"What changed?"

"What should I pay attention to?"

Charts must have:

\* clear titles

\* useful labels

\* readable axes

\* contextual summaries

\* appropriate empty states

\* responsive behavior

\* accessible colors

\* meaningful interaction

Avoid chart overload.

**==================================================**

PHASE 8 — PREMIUM EXPERIENCE

**============================**

Premium gating must feel like a product upgrade, not an arbitrary restriction.

Redesign:

\* locked analytics

\* budgeting

\* advanced reports

\* financial insights

\* premium CTA

\* subscription flow

Free users should clearly understand:

\* what is available

\* what is locked

\* what Premium provides

\* why upgrading is valuable

Do not use aggressive dark patterns.

Pricing:

Rp5.000/month

The upgrade experience should feel trustworthy and transparent.

**==================================================**

PHASE 9 — MOBILE-FIRST QUALITY

**==============================**

This is a PWA.

Mobile experience is not optional.

Audit at minimum:

\* 375px

\* 390px

\* 412px

\* 768px

\* 1024px

\* 1280px+

Ensure:

\* no horizontal overflow

\* touch targets are appropriate

\* navigation is intuitive

\* forms are easy to operate

\* tables adapt properly

\* charts remain readable

\* modal behavior is correct

\* keyboard behavior is correct

\* safe-area handling works

\* PWA viewport behavior is correct

Desktop should also feel premium rather than simply being a stretched mobile layout.

**==================================================**

PHASE 10 — MICRO-INTERACTIONS

**=============================**

Use animation selectively.

Good use:

\* transaction success

\* navigation transitions

\* modal transitions

\* loading states

\* chart reveal

\* confirmation

\* state changes

Avoid:

\* animation everywhere

\* excessive bouncing

\* distracting effects

\* slow transitions

Interaction should feel responsive and intentional.

**==================================================**

PHASE 11 — IMPLEMENTATION

**=========================**

After the UX audit and design direction are approved internally by the agent team, implement the redesign.

Do NOT rewrite backend business logic unless required for a UI/API issue.

Preserve:

\* authentication

\* authorization

\* ledger logic

\* idempotency

\* subscription state machine

\* payment verification

\* feature gating

\* database behavior

\* API contracts

The existing test baseline must remain intact.

Current known baseline:

\* 108/108 backend tests

\* 334/334 E2E tests

Any regression must be investigated, not hidden.

**==================================================**

PHASE 12 — MONOREPO ARCHITECTURE

**================================**

Only after the UI/UX direction is established, continue with the monorepo architecture.

The monorepo must preserve application independence.

Recommended conceptual structure:

apps/

nurdiansyahlabs-web/

finance-pwa/

services/

web-api/

finance-api/

packages/

design-system/

shared-contracts/

api-client/

config/

But do NOT blindly implement this structure.

Audit the existing repositories first.

Use:

\* npm/pnpm workspaces where appropriate

\* Cargo workspace for Rust

\* OpenAPI/JSON Schema for language-neutral contracts

Do NOT make Rust depend directly on TypeScript packages.

Do NOT merge PostgreSQL and SQLite.

Do NOT merge React and Vue into one frontend.

Do NOT introduce microservices unnecessarily.

**==================================================**

PHASE 13 — PRODUCTION DEPLOYMENT PROTECTION

**===========================================**

The existing NurdiansyahLabs production deployment is protected.

Do NOT break:

\`.github/workflows/deploy.yml\`

Existing behavior includes:

\* React build

\* root \`./dist/\`

\* SEO prerender

\* Puppeteer

\* SEO drift validation

\* FTP deployment to cPanel/LiteSpeed

The monorepo must support affected-project deployment.

Finance PWA changes must NOT trigger unnecessary cPanel deployment.

Finance Rust API changes must NOT trigger cPanel deployment.

NurdiansyahLabs React changes must NOT require rebuilding the finance backend.

**==================================================**

PHASE 14 — FINAL UX QUALITY GATE

**================================**

Before declaring the UI complete, perform a visual/product review.

Check:

\* Does it look like a real fintech SaaS?

\* Does it look premium?

\* Is the hierarchy immediately obvious?

\* Can a new user understand the dashboard within 5–10 seconds?

\* Can a user add a transaction quickly?

\* Are charts actually useful?

\* Are there unnecessary cards?

\* Are colors meaningful?

\* Is typography consistent?

\* Is spacing consistent?

\* Does mobile feel first-class?

\* Does desktop feel professional?

\* Does the product feel trustworthy?

\* Does it look like a template?

\* Does it look AI-generated?

\* Does every component have a clear purpose?

If the answer to any of these is "no", continue refining.

**==================================================**

EXECUTION RULE

**==============**

Do not start by restructuring the repository.

Start with:

1\. UI/UX audit

2\. Screen-by-screen findings

3\. Product design direction

4\. Design system proposal

5\. Dashboard redesign proposal

6\. Transaction UX proposal

7\. Analytics UX proposal

8\. Mobile UX proposal

9\. Visual references/inspiration

10\. Implementation plan

Then implement the UI/UX improvements.

After the UI/UX foundation is stable, proceed with monorepo architecture.

The monorepo is an architectural enabler.

The product UI/UX is the immediate product priority.

FINAL SUCCESS CRITERIA:

The Personal Finance PWA must no longer look like a basic developer-built CRUD application.

It should look and feel like a credible, modern, premium fintech SaaS product that users would trust with their financial data.

Backend correctness must remain intact.

Existing tests must remain green.

Existing production deployment must remain protected.

No destructive migration without explicit approval.

**## 2026-09-12T16:48:20Z**

USER INSTRUCTION RECEIVED:

"use this logo /home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa/frontend/public/icons"

Specific asset found:

\`/icons/Invinite\_Logo.png\` (1774 x 887 PNG with transparency, brand name 'Invinite').

Please ensure this logo is prominently integrated into:

1\. The new split-screen Login/Register experience as the primary brand asset.

2\. The Top App Bar / Desktop Sidebar.

3\. PWA manifest icons & favicon references.

4\. Replace all generic "Rp" placeholder blocks with this authentic brand logo.

**## 2026-09-13T00:59:30Z**

**# Teamwork Project Prompt — Resume Phase: Monorepo Integration & Deployment Protection**

Architect and integrate the production-grade Personal Finance SaaS (**\*\*Invinite\*\***) located at \`/home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa\` with the existing production repository **\*\*NurdiansyahLabs\*\*** located at \`/home/nurdiansyah/dev/Personal\_project\` (\`git\@github.com\:Nurdiansyah0/nurdiansyahlabs-react\`), maintaining absolute application independence, zero CI/CD breakage, and 100% test passing rates.

Working directory: \`/home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa\`

Target integration directory: \`/home/nurdiansyah/dev/Personal\_project\`

Integrity mode: development

**==================================================**

CONTEXT & CURRENT STATE (MILESTONES 1–9 COMPLETE)

**==================================================**

The user's primary UI/UX requirements have been implemented and verified:

1\. **\*\*Brand Identity\*\***: Authentic logo \`/icons/Invinite\_Logo.png\` is integrated across all surfaces.

2\. **\*\*Layout Shell\*\***: Desktop \`max-w-md\` container restriction is removed; dual-mode responsive layout shell (\`DesktopSidebar.vue\`, \`MobileBottomNav.vue\`, \`AppHeader.vue\`) is active.

3\. **\*\*Authentication\*\***: \`SplitScreenAuth.vue\` with 50/50 desktop split layout, trust pillars, and demo logins is live.

4\. **\*\*Information Hierarchy\*\***: 4-Tier dashboard (\`HomeView\.vue\`), modernized ledger (\`TransactionsView\.vue\`), decision-oriented runway metrics (\`AnalyticsView\.vue\`), and POS keypad (\`AddTransactionModal.vue\`).

5\. **\*\*Zero Raw Emojis\*\***: 100% replaced with Lucide SVG icons.

6\. **\*\*State Layer\*\***: 6 reactive Pinia stores (\`auth\`, \`wallets\`, \`transactions\`, \`categories\`, \`analytics\`, \`subscription\`).

7\. **\*\*Quality Baselines\*\***:

   \- Frontend build: \`npm run build\` -> 0 errors.

   \- Backend tests: \`cargo test --workspace\` -> 108 passed; 0 failed.

   \- E2E acceptance tests: \`bash e2e\_tests/runner.sh all\` -> 334 passed; 0 failed.

   \- Edge live health: \`https\://api.nurdiansyahlabs.com/health\` -> HTTP 200 OK.

**==================================================**

REMAINING OBJECTIVES FOR RESUMPTION

**==================================================**

**### R1. Monorepo Architecture & Application Independence (Phase 12)**

Design and establish a production-grade monorepo integration between the existing NurdiansyahLabs project (\`/home/nurdiansyah/dev/Personal\_project\`) and the Personal Finance PWA (\`/home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa\`):

\- Preserve application independence:

  \- Existing Nurdiansyahlabs React application (Vite + React + Tailwind + PHP API)

  \- Invinite Finance PWA (Vue 3 + Vite + Tailwind CSS)

  \- Invinite Rust Backend (Axum + Tokio + SQLx SQLite WAL)

\- Define clean application boundaries and shared contracts without forcing incompatible package dependencies (e.g. do NOT make Rust depend on TypeScript packages; do NOT merge SQLite with PostgreSQL; do NOT merge React with Vue).

\- Support independent development, local running, and testing for each subsystem.

**### R2. Production Deployment Protection (Phase 13)**

The existing production deployment workflow at \`/home/nurdiansyah/dev/Personal\_project/.github/workflows/deploy.yml\` is PROTECTED and MUST NOT BE BROKEN:

\- Existing workflow steps include: React build to \`./dist/\`, SEO drift validation (\`scripts/seo\_drift.py\`), PHP API packaging (\`api/\*\` to \`dist/api/\`), security hardening, and cPanel FTP deployment.

\- Configure path filtering / affected-project triggers in GitHub Actions so that:

  \- Changes to Finance PWA frontend or Rust backend do NOT trigger cPanel FTP deployment.

  \- Changes to NurdiansyahLabs React code do NOT trigger Finance PWA builds.

  \- Existing cPanel deployment behavior remains 100% identical and reliable.

**### R3. Shared Contracts & Independent Testing (Phase 14)**

\- Establish OpenAPI / JSON Schema contracts for language-neutral API specifications.

\- Ensure the test baseline for both applications remains 100% green:

  \- Invinite backend: 108/108 Rust tests

  \- Invinite E2E: 334/334 tests

  \- NurdiansyahLabs existing scripts & tests remain runnable.

**==================================================**

OPERATIONAL GUARDRAILS

**==================================================**

\- **\*\*Bounded Concurrency\*\***: To prevent API rate limit exhaustion (RESOURCE\_EXHAUSTED 429), run sequentially or with a maximum of 2 concurrent subagents. Avoid high-frequency polling crons.

\- **\*\*Safety\*\***: Do not delete existing code or perform destructive migrations without creating backup branches.

\- **\*\*SELinux & Edge\*\***: Keep SELinux Enforcing on edge devices and maintain localhost-only bindings for internal services.

**## Acceptance Criteria**

**### Monorepo Structure & Boundaries**

\- [ ] Monorepo workspace configuration (npm/pnpm workspaces or Cargo workspace) clearly separates \`apps/\`, \`services/\`, and \`packages/\` or maintains clean project references without breaking directory structures.

\- [ ] React and Vue apps can be built and previewed independently with zero cross-runtime interference.

\- [ ] Rust backend builds independently (\`cargo check\`, \`cargo test\`) without Node/npm dependency couplings.

**### Production CI/CD Protection**

\- [ ] \`.github/workflows/deploy.yml\` in \`nurdiansyahlabs-react\` is audited and protected with appropriate path filters (\`paths:\` / \`paths-ignore:\`) so cPanel deployment only triggers on relevant web changes.

\- [ ] Finance PWA CI/CD workflow is isolated to its own workflow definition without coupling to FTP credentials.

**### Quality & Regression Verification**

\- [ ] Invinite PWA frontend build succeeds (\`npm run build\`).

\- [ ] Invinite Rust backend test suite passes (108/108 tests green).

\- [ ] Invinite E2E suite passes (334/334 tests green).

\- [ ] Final architecture report and integration guide are documented.

**## Follow-up — 2026-09-13T02:15:42Z**

Implement an on-demand 7-day Premium Free Trial model and integrate Direct DANA Open API (developer.dana.id) into the Invinite Personal Finance SaaS to process DANA checkout payments (Rp 5.000/month) and securely receive live asynchronous payment success webhooks.

Working directory: \`/home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa\`

Target production domain: \`https\://api.nurdiansyahlabs.com\`

Integrity mode: development

**## Requirements**

**### R1. On-Demand 7-Day Premium Trial Lifecycle**

\- Provide an on-demand 7-day free trial activation flow in the frontend and backend without requiring upfront payment or credit card details.

\- Grant full Pro feature access (advanced analytics, financial health scoring, multi-month runway forecasts, custom budgeting, and encrypted exports) during the 7-day trial period.

\- Accurately track remaining trial duration in real time. Upon expiration, prompt the user for the Rp 5.000/month subscription or transition gracefully back to the Free tier without data loss.

**### R2. Direct DANA Open API Integration (developer.dana.id)**

\- Implement direct integration with DANA Open API / Enterprise standards:

  \- Generate DANA checkout payment sessions / QRIS orders for subscription purchase (Rp 5.000).

  \- Implement the asynchronous transaction notification webhook endpoint at \`POST /api/v1/webhooks/dana\`.

  \- Cryptographically verify DANA incoming request signatures (RSA-SHA256 / SNAP Open API standard) using public key cryptography to protect against tampering and forgery.

  \- Implement idempotent deduplication to safely ignore repeated or replayed webhook callbacks.

  \- Respond with the standard DANA response acknowledgement payload (e.g. \`2005600 Successful\`).

**### R3. Atomic Subscription Settlement & Audit Logging**

\- Upon receiving a verified success notification from DANA, atomically transition the user's subscription from \`trialing\` or \`free\` to \`active\` Premium.

\- Extend subscription period by 30 days and record an immutable audit trail entry in the database.

**### R4. Production Deployment & Live Edge Verification**

\- Ensure all new endpoints and UI components are compiled, deployed, and verified live on the Samsung Galaxy A20s edge server serving \`https\://api.nurdiansyahlabs.com\`.

\- Maintain 100% test pass rate across the existing test suite (Rust unit/integration tests and E2E acceptance tests).

**## Acceptance Criteria**

**### 7-Day Trial Flow**

\- [ ] Users can trigger trial activation via the UI modal and immediately receive active Pro permissions (\`analytics.advanced\`, \`budgeting\`, \`reports.advanced\`).

\- [ ] \`GET /api/v1/subscription\` accurately reflects \`status: "trialing"\`, \`is\_premium: true\`, and the remaining days integer.

\- [ ] Trial cannot be activated multiple times by the same account.

**### DANA Open API & Webhook Processing**

\- [ ] \`POST /api/v1/subscriptions/checkout\` supports provider \`dana\` and returns a valid DANA checkout URL / transaction reference.

\- [ ] \`POST /api/v1/webhooks/dana\` rejects notifications with missing or invalid cryptographic signatures with HTTP 401/400.

\- [ ] \`POST /api/v1/webhooks/dana\` accepts valid success payloads, activates Premium tier for the user, and returns the standard DANA success acknowledgement.

\- [ ] Repeated delivery of the same DANA notification is deduplicated idempotently with HTTP 200 without duplicate period extensions.

**### Edge Production & Regression Suite**

\- [ ] Frontend builds with 0 warnings/errors (\`npm run build\`).

\- [ ] Backend builds and passes 100% of workspace tests (\`cargo test --workspace\`).

\- [ ] E2E acceptance test runner passes 100% of test assertions (\`bash e2e\_tests/runner.sh all\`).

\- [ ] Live edge server responds with HTTP 200 on all newly introduced endpoints.

**## 2026-09-13T15:00:36Z**

**# Teamwork Project Prompt**

Perform a comprehensive technical audit and architectural analysis of the existing Personal Finance PWA repository before any modification. Strictly read-only analysis delivering an evidence-based audit report saved to \`AUDIT\_REPORT.md\` with verified findings and prioritized recommendations.

Working directory: /home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa

Integrity mode: development

**## Requirements**

**### R1. Comprehensive Multi-Perspective Technical Audit**

Perform a thorough, evidence-based technical audit of the repository covering all layers:

\- **\*\*Repository Structure & Build\*\***: Directory structure, package manifests, build scripts, configuration, and dependencies.

\- **\*\*Frontend Architecture\*\***: Vue 3, Pinia, Tailwind CSS, Vite, PWA service workers/manifest, routing, state management, UI consistency, and dead/duplicated components.

\- **\*\*Backend Architecture\*\***: Rust/Axum API endpoints, service layers, middleware, error handling, validation, and domain logic.

\- **\*\*Database & Financial Data Integrity\*\***: Schema, migrations, relationships, transactions, indexes, and financial domain invariants (IDR Rupiah integer precision vs floating point, atomic balance deltas, cash flow engine).

\- **\*\*Security & Authorization\*\***: Authentication flow, session/token lifecycle, CORS/CSRF, security headers, input validation, and credential exposure. Classify issues as \`CRITICAL\`, \`HIGH\`, \`MEDIUM\`, \`LOW\`, or \`INFORMATIONAL\`.

\- **\*\*DevOps, Testing & Deployment\*\***: CI/CD, deployment configs, Docker/start scripts, unit/integration/E2E test coverage, and deployment readiness (\`NOT READY\`, \`DEVELOPMENT READY\`, \`STAGING READY\`, \`PRODUCTION READY\`).

Every finding must be supported by repository evidence (file path and line number or code snippet) and classified as \`FACT\`, \`INFERENCE\`, \`RISK\`, \`RECOMMENDATION\`, or \`UNKNOWN\`.

**### R2. Strict Non-Modification Constraint (Read-Only)**

Do not edit, delete, or rename existing source files, configurations, database files, schemas, or dependencies. Do not commit to git, push branches, or alter deployment state. All diagnostic actions must be strictly read-only. The only permitted file write is the final report artifact \`AUDIT\_REPORT.md\` in the repository root.

**### R3. Deliverable: Structured Audit Report (\`AUDIT\_REPORT.md\`)**

Deliver the comprehensive audit report written to \`AUDIT\_REPORT.md\` in the repository root adhering to the structured framework:

\- **\*\*A. Executive Summary\*\***: Maximum 10–15 key findings.

\- **\*\*B. Current Architecture\*\***: Explain the actual system.

\- **\*\*C. Technology Stack Inventory\*\***: Verified technologies and versions table (Layer, Technology, Version, Status, Evidence).

\- **\*\*D. Feature Status Matrix\*\***: Feature, Implemented, Partial, Missing, Broken, Evidence.

\- **\*\*E. Security Findings\*\***: Prioritized security matrix (Severity, Finding, Location, Evidence, Impact, Recommendation).

\- **\*\*F. Database Findings\*\***: Integrity, performance, transaction boundaries, and financial precision.

\- **\*\*G. Backend Findings\*\***: API surface audit (Method, Path, Auth, Request, Response, Logic, Error handling).

\- **\*\*H. Frontend Findings\*\***: Architecture, UX, performance, maintainability.

\- **\*\*I. DevOps Findings\*\***: CI/CD, deployment, infrastructure.

\- **\*\*J. Testing Findings\*\***: Current testing maturity, test coverage, and gaps.

\- **\*\*K. Technical Debt Register\*\***: Categorized by CRITICAL/HIGH/MEDIUM/LOW with location, impact, and action.

\- **\*\*L. Architectural Risk Register\*\***: Prioritized list with severity, probability, impact, and mitigation.

\- **\*\*M. What Should NOT Be Changed\*\***: Concrete \`PRESERVE\` list of functional components that must remain intact.

\- **\*\*N. What Should Be Changed & Target Architecture\*\***: Prioritized roadmap (P0 Blockers, P1 Critical, P2 Important, P3 Optimization) with change impact analysis.

\- **\*\*O. Implementation Plan\*\***: Step-by-step plan for the next phase.

\- **\*\*Section 25 Final Decision\*\***:

  \`\`\`text

  PROJECT STATUS: [STATUS]

  ARCHITECTURAL HEALTH: [HEALTH]

  SECURITY POSTURE: [STATUS]

  PRODUCTION READINESS: [STATUS]

  PRIMARY BLOCKERS: 1. ... 2. ... 3. ...

  RECOMMENDED NEXT ACTION: ...

  \`\`\`

**## Acceptance Criteria**

**### Audit Deliverable & Format**

\- [ ] \`AUDIT\_REPORT.md\` is generated in \`/home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa/AUDIT\_REPORT.md\` containing all required sections A through O and the Section 25 Final Decision block.

\- [ ] Every major architectural, security, and data integrity finding provides explicit repository evidence (exact file paths and line numbers / snippets).

\- [ ] Technology stack inventory and feature status tables are fully populated with evidence tags (\`FACT\`, \`INFERENCE\`, etc.) and no speculative placeholders.

\- [ ] Clear categorization of deployment readiness (\`NOT READY\`, \`DEVELOPMENT READY\`, \`STAGING READY\`, or \`PRODUCTION READY\`) backed by technical justification.

\- [ ] A concrete \`PRESERVE\` list explicitly identifying functional components that must remain intact.

**### Integrity & System State**

\- [ ] \`git status\` verifies no existing source files, test files, configs, or dependencies were altered or deleted (only \`AUDIT\_REPORT.md\` added).

\- [ ] No migrations were run and no database files were modified.

**---**

**## Detailed Audit Specifications & Agent Distribution**

**### Analysis Principles**

\- Repository is the source of truth.

\- Verify before concluding; do not rely on assumptions.

\- Do not rewrite functioning components without evidence.

\- Every major finding must include evidence from the repository (\`FACT\`, \`INFERENCE\`, \`RISK\`, \`RECOMMENDATION\`, \`UNKNOWN\`).

\- Avoid generic statements; state exact locations and provide evidence.

**### Multi-Agent Work Distribution**

\- **\*\*Agent 1 — Repository Auditor\*\***: Analyze repository structure, package manifests, build system, deployment files, and environment configs.

\- **\*\*Agent 2 — Frontend Auditor\*\***: Analyze Vue 3/Pinia/Tailwind/Vite, component architecture, state management, API integration, PWA implementation, responsiveness, accessibility, UI consistency, and dead components.

\- **\*\*Agent 3 — Backend Auditor\*\***: Analyze Rust/Axum endpoints, service layers, middleware, auth/authz, validation, error handling, and business logic.

\- **\*\*Agent 4 — Database & Data Auditor\*\***: Analyze schema, migrations, models, relations, indexes, transactions, and monetary precision (integer Rupiah vs floats, atomic deltas).

\- **\*\*Agent 5 — Security Auditor\*\***: Defensive security review (auth, session management, secrets, CORS, CSRF, XSS, injection, headers). Classify findings (CRITICAL/HIGH/MEDIUM/LOW/INFORMATIONAL).

\- **\*\*Agent 6 — DevOps & Deployment Auditor\*\***: Analyze Docker, CI/CD, deployment scripts, web server configs, health checks, rollback/backup strategy.

\- **\*\*Agent 7 — Product & Requirements Auditor\*\***: Compare current implementation vs expected product behavior, identifying implemented, partial, broken, and missing features.

\- **\*\*Agent 8 — Architecture Auditor\*\***: High-level architecture model, coupling, cohesion, bottlenecks, single points of failure.

**## 2026-09-15T11:30:45Z**

This is a single self-contained fix; keep it small and focused.

Resolve the favicon rendering issues, concise browser title, and complete the production-ready UI flows for authentication, specifically the email login flow and the "Lupa Akun" (account recovery / forgot password) view in FinRep.

Working directory: /home/nurdiansyah/teamwork\_projects/personal\_finance\_pwa

Integrity mode: development

**## Requirements**

**### R1. Favicon Assets & Browser Tab Title Resolution**

\- Replace the corrupted/black \`favicon.ico\` with a crisp multi-resolution ICO file (16x16, 32x32, 48x48) generated accurately from \`favicon.svg\` with emerald and mint palette and transparent background.

\- Update \`index.html\` document title from the truncated 57-character string to a concise, non-overflowing title (e.g. \`FinRep — Dasbor Finansial\`).

\- Ensure all related touch and PWA icons (\`apple-touch-icon.png\`, \`icon-192.png\`, \`icon-512.png\`) render the full shield and arrow logo with appropriate \`#09090b\` background padding.

**### R2. "Lupa Akun" (Account Recovery) UI Flow**

\- Implement a dedicated, polished "Lupa Akun" (Forgot Password / Account Recovery) UI accessible from the authentication screen.

\- Include clear input for user's registered email, validation handling, friendly microcopy matching FinRep tone, and a seamless return link to "Masuk ke Akun Saya".

\- Include a simulated or actual submission state displaying confirmation feedback ("Tautan pemulihan akun telah dikirimkan ke email Anda") with an option to resend or back to login.

**### R3. Production-Ready Authentication UI Polish**

\- Ensure login and recovery forms adhere to production quality: complete loading states, inline customer-safe error handling, password visibility toggle, and keyboard accessibility (Enter to submit).

\- Guarantee 100% viewport fitting on desktop screens (down to 600px height) without cut-off elements, and fluid mobile ergonomics with safe scrolling.

**## Acceptance Criteria**

**### Asset Integrity & Branding**

\- [ ] \`favicon.ico\` contains valid 16x16, 32x32, and 48x48 icons with non-black, emerald and mint brand colors identical to \`favicon.svg\`.

\- [ ] Browser \`\<title>\` in \`frontend/index.html\` is under 30 characters and does not get truncated on standard browser tabs.

\- [ ] \`npm run build\` in \`frontend/\` succeeds without compilation errors or missing asset warnings.

**### Authentication & Recovery Flow**

\- [ ] Users can navigate from the Login screen to "Lupa Akun" and back without broken routing or layout shifts.

\- [ ] Submitting the "Lupa Akun" form with a valid email triggers a clear confirmation feedback state.

\- [ ] The entire layout on desktop (1366x600, 1280x720, 1920x1080) fits without clipping or invisible scrollbars.

\- [ ] Touch targets and mobile viewport ergonomics are preserved on mobile screens.

---

# 2026-09-16 — ARCHITECTURE UPDATE: NATIVE ANDROID + AUTOMATIC FINANCIAL INGESTION

The product architecture is now explicitly **PWA-first UI + Native Android Capability Layer + Rust/Axum Modular Monolith**.

This update supersedes only the assumptions necessary to support native Android capabilities, subscription activation, automatic financial transaction ingestion, realtime behavior, and battery-aware synchronization. Existing financial correctness, security, test, deployment, and UI requirements remain mandatory.

## 1. TARGET PRODUCT MODEL

The product is no longer treated as only a browser PWA.

The target product consists of:

```text
                         INvinite / FinRep
                               │
              ┌────────────────┼────────────────┐
              │                │                │
             WEB            ANDROID             iOS
              │                │                │
             PWA       Kotlin Native Shell      PWA
                              │
                           WebView
                              │
                         Shared PWA UI
                              │
                    Native Capability Layer
                              │
             ┌────────────────┼────────────────┐
             │                │                │
        Notifications        SMS          Native Push
             │                │                │
             └────────────────┼────────────────┘
                              │
                         HTTPS / WS
                              │
                              ▼
                  Rust + Axum Modular Monolith
                              │
                              ▼
                         SQLite WAL
```

The PWA remains the primary UI implementation and must not be duplicated as a separate native UI.

The Android application uses Kotlin for capabilities that require Android platform integration. Kotlin is not responsible for duplicating business logic that already belongs to the Rust backend.

## 2. BACKEND ARCHITECTURE — MODULAR MONOLITH

The backend remains a **single Rust/Axum/Tokio/SQLx deployable application**.

Do NOT convert the backend into microservices.

Do NOT introduce backend-to-backend gRPC merely for architectural appearance or theoretical performance.

Use in-process module/service calls and domain events inside the monolith.

Target conceptual structure:

```text
backend/
└── src/
    ├── modules/
    │   ├── auth/
    │   ├── users/
    │   ├── accounts/
    │   ├── transactions/
    │   ├── categories/
    │   ├── budgets/
    │   ├── goals/
    │   ├── subscriptions/
    │   ├── payments/
    │   ├── ingestion/
    │   ├── notifications/
    │   ├── realtime/
    │   └── reporting/
    │
    ├── infrastructure/
    ├── config/
    └── main.rs
```

The modular boundaries must remain explicit even though all modules run inside one process.

Example:

```text
TransactionService
      │
      ├── AccountService
      ├── CategoryService
      └── Domain Event: TransactionCreated
                         │
                         ▼
                    Realtime Module
```

This is an in-process interaction, not HTTP or gRPC.

## 3. CLIENT COMMUNICATION ARCHITECTURE

Use the protocol according to the actual communication problem.

### HTTP / HTTPS

HTTP/HTTPS is the default application API protocol.

Use it for:

- authentication;
- CRUD;
- transaction creation and editing;
- account management;
- categories;
- subscription status;
- checkout;
- synchronization;
- reports;
- settings.

### WebSocket

WebSocket is the realtime channel for active application sessions.

Use it for:

- transaction-created events;
- transaction-updated events;
- balance changes;
- active dashboard updates;
- realtime notifications;
- synchronization hints;
- long-running operation progress.

The realtime connection must be lifecycle-aware.

```text
APP FOREGROUND
PWA/WebView
     ║
     ║ WebSocket
     ║
Rust/Axum
```

When the application is backgrounded, do not maintain a permanent high-frequency realtime connection merely to simulate realtime.

```text
APP BACKGROUND
Android
   │
   ├── Push / OS background facilities
   └── No unnecessary permanent WebSocket
```

When the application resumes:

```text
resume
  ↓
sync from last cursor
  ↓
apply delta
  ↓
re-establish realtime connection
```

### gRPC

gRPC is NOT part of the primary communication architecture at this stage.

The backend is a monolith, therefore backend modules communicate in-process.

gRPC may only be introduced later if a module is deliberately extracted into an independently deployed service.

Do not add gRPC to the Android/PWA path merely because it is binary or theoretically efficient.

For the current architecture, battery efficiency is primarily achieved through lifecycle-aware connectivity, delta synchronization, reduced network activity, and OS-native background mechanisms.

## 4. KOTLIN NATIVE ANDROID LAYER

The Android application must use Kotlin as a **native capability and platform integration layer**.

Kotlin responsibilities include:

- hosting the shared PWA through WebView;
- Android lifecycle management;
- notification ingestion;
- supported SMS integration;
- native push handling;
- background synchronization coordination;
- secure native storage where appropriate;
- connectivity state;
- tightly scoped JavaScript bridge;
- native subscription/activation entry flow;
- communication between native capabilities and PWA state.

Kotlin must NOT duplicate:

- ledger calculation;
- account balance calculation;
- subscription business rules;
- authorization rules;
- financial reconciliation;
- transaction persistence rules.

Those remain authoritative in Rust.

## 5. PWA ↔ KOTLIN BRIDGE

Create a versioned, capability-based native bridge.

```text
PWA
 │
 │ NativeBridge.request(...)
 ▼
Kotlin
 │
 ├── notification capability
 ├── sync capability
 ├── push capability
 ├── secure-storage capability
 └── lifecycle capability
 │
 ▼
PWA state / event
```

Do not expose unrestricted native methods to JavaScript.

Native bridge calls must:

- validate arguments;
- enforce capability authorization;
- return structured success/error responses;
- be versioned;
- avoid leaking sensitive raw device data;
- remain compatible with the web/PWA runtime by providing safe fallbacks.

## 6. FIRST-LAUNCH BUSINESS LOGIC — ANNUAL SUBSCRIPTION GATE

The downloaded Android application must have a native first-launch activation layer.

The initial Android launch flow is:

```text
APK downloaded
      ↓
Kotlin Native Shell
      ↓
First Launch
      ↓
Authentication / Account Identification
      ↓
Subscription Status Verification
      ↓
┌───────────────────────────────┐
│                               │
│ ACTIVE / VALID                │
│       ↓                       │
│ PWA + Pro Native Features    │
│                               │
├───────────────────────────────┤
│ NOT SUBSCRIBED / EXPIRED      │
│       ↓                       │
│ Annual Subscription Layer     │
│       ↓                       │
│ Payment / Activation           │
│       ↓                       │
│ Backend Verification           │
│       ↓                       │
│ PWA + Pro Native Features     │
└───────────────────────────────┘
```

The annual subscription layer is a **native Android entry/activation responsibility**.

However, Kotlin must NOT be the authoritative source of entitlement.

The backend remains authoritative.

```text
Subscription / Payment Provider
            ↓
Rust Subscription Module
            ↓
Entitlement
            ↓
Kotlin + PWA
```

Never rely on a permanently trusted local `isSubscribed=true` flag.

Local state may accelerate startup, but protected capabilities must ultimately be authorized by the backend.

## 7. SUBSCRIPTION MODEL

The existing product requirements contain Rp5.000/month Premium and an on-demand 7-day Premium trial.

The implementation must reconcile these into one authoritative subscription/entitlement model rather than creating multiple independent subscription systems.

Use explicit states:

```text
FREE
TRIALING
ACTIVE
GRACE
CANCELLED
EXPIRED
PENDING
UNVERIFIED
```

Use explicit feature entitlements:

```text
PRO_ACCESS
AUTO_TRANSACTION_INGESTION
NOTIFICATION_INGESTION
SMS_INGESTION
EMAIL_INGESTION
ADVANCED_ANALYTICS
BUDGETING
ADVANCED_REPORTING
ENCRYPTED_EXPORTS
```

The backend maps subscription state to entitlements.

Kotlin and PWA consume the resulting capability set.

## 8. AUTOMATIC FINANCIAL TRANSACTION INGESTION — PRO FEATURE

The Pro product should automatically detect financial events from supported external sources.

The system must be designed as a **multi-source, failure-resistant ingestion pipeline**.

Do NOT claim that an external source can never fail.

Instead, implement deterministic fallback and unresolved-event handling.

Target pipeline:

```text
Financial Event
      │
      ├──────────────┬──────────────┐
      ▼              ▼              ▼
 Notification       SMS           Gmail
      │              │              │
      └──────────────┼──────────────┘
                     ▼
               Source Adapter
                     ▼
                  Parser
                     ▼
                Normalizer
                     ▼
                 Validator
                     ▼
             Confidence Engine
                     ▼
              Deduplication
                     ▼
             Transaction Candidate
                     ▼
              Domain Validation
                     ▼
                Persistence
```

## 9. ANDROID NOTIFICATION INGESTION

When the user explicitly grants the appropriate Android notification access, Kotlin may process relevant financial notifications.

Example raw event:

```text
BCA
Debit Rp125.000
```

Normalize to:

```json
{
  "amount": 125000,
  "direction": "expense",
  "currency": "IDR",
  "provider": "bca",
  "source": "notification"
}
```

Do NOT assume every notification is a financial transaction.

The parser must identify:

- transaction direction;
- amount;
- currency;
- provider;
- timestamp where available;
- merchant/description where available;
- external reference where available.

Only the minimum required structured fields should be retained.

## 10. SMS FALLBACK

SMS ingestion is a conditional capability.

The implementation must respect Android platform restrictions, permission requirements, and distribution policies.

Do not design the product around an assumption that arbitrary SMS access is always available.

If SMS ingestion is unavailable:

```text
SMS unavailable
      ↓
Continue to Gmail / other supported source
```

The system must fail gracefully.

## 11. GMAIL FALLBACK

Gmail is an account-connected fallback source.

Use an explicit OAuth authorization flow and the minimum practical scope required for the feature.

Do NOT mirror the user's entire mailbox.

Target flow:

```text
User
 ↓
Google OAuth
 ↓
Authorized Gmail access
 ↓
Targeted financial message query
 ↓
Financial email parser
 ↓
Canonical transaction candidate
 ↓
Validation / deduplication
 ↓
Backend
```

Gmail-derived data must be minimized and retained only as necessary for transaction processing, reconciliation, or audit requirements.

## 12. INGESTION SOURCE PRIORITY

Use a deterministic fallback strategy.

Default priority:

```text
1. Android financial notification
2. SMS, when legitimately available
3. Gmail connected account
4. Manual confirmation / manual entry
```

This is a source preference, not a guarantee that every device/user supports every source.

## 13. TRANSACTION INTELLIGENCE

All external data must converge to one canonical domain representation.

Target schema:

```text
transaction_id
amount
currency
direction
occurred_at
provider
merchant
account
source
external_reference
confidence
ingestion_id
```

Provider-specific parsing logic must remain isolated inside source adapters/parsers.

The rest of the application must consume the canonical domain representation.

## 14. CONFIDENCE AND HUMAN REVIEW

Automatic parsing must not blindly create financial records.

Use a confidence pipeline:

```text
Raw Event
    ↓
Parse
    ↓
Validate
    ↓
Confidence
    │
    ├── HIGH
    │     ↓
    │  Auto-create
    │
    ├── MEDIUM
    │     ↓
    │  User confirmation
    │
    └── LOW
          ↓
      Unresolved / reject
```

Confidence is an ingestion-quality signal. It is not a substitute for financial-domain validation.

## 15. DEDUPLICATION AND IDEMPOTENCY

The same transaction may arrive through multiple sources:

```text
Notification ─┐
SMS ──────────┼──► Deduplication ──► ONE transaction
Gmail ────────┘
```

Deduplication must use multiple signals where available:

- provider;
- external reference;
- amount;
- direction;
- timestamp/window;
- merchant;
- account;
- normalized content fingerprint.

The ingestion pipeline must be idempotent.

Repeated processing of the same event must never create duplicate financial transactions.

The existing `Idempotency-Key` requirements remain mandatory for API transaction creation.

## 16. REALTIME INGESTION FEEDBACK

When an automatically detected transaction is persisted, active clients should receive a realtime event.

```text
Notification / SMS / Gmail
          ↓
    Ingestion Engine
          ↓
      Transaction
          ↓
   TransactionCreated
          ↓
      WebSocket
          ↓
   Active PWA/WebView
          ↓
      UI updates
```

Background clients should use push/synchronization rather than requiring a permanently active WebSocket.

## 17. OFFLINE AND DELTA SYNC

Implement cursor-based synchronization.

Example:

```text
Client cursor = 1820

GET /sync?cursor=1820

Response:
cursor = 1827
changes = [...]
```

Do not repeatedly download complete transaction histories when only a small number of records changed.

Large lists must use server-side pagination and filtering.

Examples:

```text
GET /transactions?page=1&limit=20
GET /transactions?from=2026-09-01&to=2026-09-30
GET /transactions?cursor=...
```

Financial mutations must be retry-safe using stable client IDs and/or idempotency keys.

## 18. BATTERY AND PERFORMANCE REQUIREMENTS

Performance must be evaluated at the system level.

Do not equate "binary protocol" with "battery efficient".

Battery optimization must prioritize:

- lifecycle-aware WebSocket usage;
- no unnecessary permanent background connections;
- push-based wake-up where appropriate;
- delta synchronization;
- pagination;
- minimal payloads;
- conservative reconnect behavior;
- avoiding aggressive polling;
- OS-native background scheduling;
- local caching of non-sensitive application state;
- idempotent synchronization.

Foreground target:

```text
Fast interaction
+
Realtime WebSocket
```

Background target:

```text
Low network activity
+
Push / OS background mechanism
+
Delta sync on resume
```

## 19. SECURITY AND PRIVACY FOR FINANCIAL INGESTION

Financial notifications, SMS, and email are sensitive data.

Mandatory principles:

- explicit user consent;
- least privilege;
- data minimization;
- encrypted sensitive credentials/tokens;
- no complete mailbox mirroring;
- no unnecessary raw SMS storage;
- no unnecessary raw notification storage;
- no raw financial message content in logs;
- strict server-side authorization;
- auditable source activation/revocation;
- defensive parsing of untrusted external text;
- secure native-to-JavaScript bridge;
- no secrets in PWA bundles.

Gmail authorization must be treated as an account integration, not as unrestricted application access.

## 20. IOS BEHAVIOR

The baseline iOS experience remains PWA-based.

If the user installs the application through Safari/Add to Home Screen:

```text
Safari
  ↓
Add to Home Screen
  ↓
iOS PWA
```

It must not be incorrectly represented as a Kotlin native application.

Android is the platform receiving the Kotlin Native Capability Layer.

The shared PWA UI and backend API remain cross-platform.

## 21. UPDATED PRODUCT ARCHITECTURE

The target architecture is therefore:

```text
                           USERS
                             │
              ┌──────────────┼──────────────┐
              │              │              │
             Web          Android          iOS
              │              │              │
             PWA       Kotlin Native       PWA
                            Shell
                              │
                           WebView
                              │
                         Shared PWA UI
                              │
                    Native Capability Layer
                              │
             ┌────────────────┼────────────────┐
             │                │                │
       Notifications         SMS          Native Push
             │                │                │
             └────────────────┼────────────────┘
                              │
                         HTTPS / WS
                              │
                              ▼
                   Rust + Axum + Tokio
                    MODULAR MONOLITH
                              │
        ┌─────────────────────┼────────────────────┐
        │                     │                    │
     Auth/Authz          Transactions         Subscription
        │                     │                    │
        ├─────────────────────┼────────────────────┤
        │                     │                    │
     Ingestion           Realtime             Reporting
        │                     │                    │
        └─────────────────────┼────────────────────┘
                              │
                         SQLite WAL
```

## 22. UPDATED IMPLEMENTATION ORDER

The implementation sequence must now be:

1. Audit the current repository and preserve working behavior.
2. Establish the authoritative design system.
3. Stabilize responsive PWA UI.
4. Preserve backend financial correctness.
5. Implement subscription/entitlement model.
6. Implement Android Kotlin shell.
7. Implement secure PWA ↔ Kotlin capability bridge.
8. Implement native notification ingestion.
9. Implement conditional SMS ingestion.
10. Implement Gmail OAuth/integration fallback.
11. Implement normalization, confidence, validation, and deduplication.
12. Implement realtime WebSocket transaction events.
13. Implement push/background synchronization.
14. Implement cursor-based delta synchronization.
15. Add comprehensive unit/integration/E2E coverage.
16. Verify security and data-minimization requirements.
17. Only then continue broader monorepo integration work.

Do not introduce microservices or gRPC unless a measurable future requirement creates an independent service boundary.

## 23. UPDATED ACCEPTANCE CRITERIA

### Native Android

- [ ] Android application is implemented in Kotlin.
- [ ] Kotlin acts as native capability layer, not a duplicated UI implementation.
- [ ] PWA is rendered through WebView.
- [ ] Native bridge is capability-based and secure.
- [ ] First-launch annual subscription layer is handled by the native Android shell.
- [ ] Subscription entitlement remains backend-authoritative.
- [ ] Reinstallation can restore entitlement after account authentication.

### Financial Ingestion

- [ ] Notification ingestion works when the user grants the required Android capability.
- [ ] SMS ingestion is implemented only where Android/platform policy permits it.
- [ ] Gmail fallback uses explicit OAuth authorization.
- [ ] External events are normalized into a canonical transaction schema.
- [ ] Duplicate events across notification/SMS/Gmail do not create duplicate transactions.
- [ ] Low-confidence events are not silently converted into financial records.
- [ ] Ingestion processing is idempotent.
- [ ] Raw financial messages are not unnecessarily persisted or logged.

### Realtime and Battery

- [ ] Active foreground sessions receive relevant transaction events through WebSocket.
- [ ] Background operation does not depend on a permanently maintained WebSocket.
- [ ] Resume triggers cursor-based delta synchronization.
- [ ] Reconnect behavior is bounded and battery-aware.
- [ ] Large transaction collections use pagination/filtering.
- [ ] Network payloads are minimized.

### Backend Architecture

- [ ] Rust backend remains a Modular Monolith.
- [ ] Backend modules communicate in-process.
- [ ] No unnecessary gRPC layer is introduced.
- [ ] Existing financial calculations, idempotency, auth, authorization, payment verification, and database integrity remain intact.
- [ ] Existing test baseline remains green.

### Platform

- [ ] Web remains a PWA.
- [ ] Android uses Kotlin native capabilities.
- [ ] iOS baseline remains PWA when installed from Safari.
- [ ] All clients use the same authoritative backend and domain model.

## 24. NON-GOALS

Do NOT:

- convert the backend to microservices;
- add gRPC solely for speed claims;
- duplicate the entire Vue UI in Kotlin;
- require a permanent WebSocket in the background;
- assume SMS access is universally available;
- mirror Gmail mailboxes;
- store raw financial messages unnecessarily;
- make local Kotlin subscription state authoritative;
- bypass backend authorization;
- weaken financial idempotency to improve UX;
- break existing deployment workflows;
- perform destructive migrations without explicit approval.

## 25. FINAL ARCHITECTURAL DECISION

The authoritative target is:

```text
PWA-FIRST UI
        +
KOTLIN NATIVE ANDROID CAPABILITY LAYER
        +
ANNUAL SUBSCRIPTION / ENTITLEMENT GATE
        +
MULTI-SOURCE FINANCIAL INGESTION
        +
RUST/AXUM MODULAR MONOLITH
        +
SQLITE WAL
        +
HTTPS API
        +
FOREGROUND WEBSOCKET REALTIME
        +
PUSH / BACKGROUND DELTA SYNC
        +
STRICT IDEMPOTENCY / DEDUPLICATION
        +
DATA MINIMIZATION
```

The architecture must optimize for **fast perceived interaction, realtime behavior while active, low unnecessary battery/network consumption, reliable automatic transaction capture, financial correctness, and maintainable single-application deployment**.

