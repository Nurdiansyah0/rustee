use axum::Router;
use crate::api::handlers::ingestion::ingestion_router;
use crate::api::AppState;

/// Registers ingestion pipeline routes under `/ingestion`.
/// Baseline endpoints per Master Specification v3.1.0:
/// - POST /ingestion/notification: Ingest notification payload with Idempotency-Key
/// - POST /ingestion/sms: Ingest SMS payload with Idempotency-Key
/// - POST /ingestion/gmail: Ingest Gmail payload with Idempotency-Key
/// - GET /ingestion/candidates: Returns pending candidates requiring confirmation
/// - POST /ingestion/candidates/:id/confirm: User confirms candidate, atomically committing to ledger
/// - POST /ingestion/candidates/:id/reject: Rejects candidate
pub fn register_ingestion_routes(router: Router<AppState>) -> Router<AppState> {
    router.nest("/ingestion", ingestion_router())
}
