use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;

use crate::api::AppState;
use crate::domain::accounting::{
    calculate_tax_integer, Account, JournalEntryWithLines, TaxCalculationResult,
};
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::repository::accounting_repo::TrialBalanceDto;
use crate::service::accounting_service::{
    AccountsListResponse, CreateAccountRequest, DeleteAccountResponse, JournalsListResponse,
    PostJournalLineRequest, PostJournalRequest, ReverseJournalRequest,
};

/// GET /api/v1/accounting/accounts
pub async fn list_accounts_handler(
    ctx: TenantContext,
    State(state): State<AppState>,
) -> Result<Json<AccountsListResponse>, AppError> {
    let svc = state.accounting_service();
    let res = svc.list_accounts(&ctx).await?;
    Ok(Json(res))
}

/// POST /api/v1/accounting/accounts
pub async fn create_account_handler(
    ctx: TenantContext,
    State(state): State<AppState>,
    Json(payload): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<Account>), AppError> {
    let svc = state.accounting_service();
    let account = svc.create_account(&ctx, payload).await?;
    Ok((StatusCode::CREATED, Json(account)))
}

/// DELETE /api/v1/accounting/accounts/{code}
pub async fn delete_account_handler(
    ctx: TenantContext,
    Path(code): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<DeleteAccountResponse>, AppError> {
    let svc = state.accounting_service();
    let res = svc.delete_account(&ctx, &code).await?;
    Ok(Json(res))
}

/// GET /api/v1/accounting/journals
pub async fn list_journals_handler(
    ctx: TenantContext,
    State(state): State<AppState>,
) -> Result<Json<JournalsListResponse>, AppError> {
    let svc = state.accounting_service();
    let res = svc.list_journals(&ctx).await?;
    Ok(Json(res))
}

/// POST /api/v1/accounting/journals
pub async fn post_journal_handler(
    ctx: TenantContext,
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<JournalEntryWithLines>), AppError> {
    let json_val: Value = serde_json::from_slice(&body).map_err(|_| {
        AppError::BadRequest(
            "Invalid JSON payload for journal entry".to_string(),
            "INVALID_JOURNAL",
        )
    })?;

    if !json_val.is_object() {
        return Err(AppError::BadRequest(
            "Journal payload must be a JSON object".to_string(),
            "INVALID_JOURNAL",
        ));
    }

    let lines_val = match json_val.get("lines") {
        Some(Value::Array(arr)) => arr,
        _ => {
            return Err(AppError::BadRequest(
                "Journal lines cannot be empty".to_string(),
                "INVALID_JOURNAL_LINES",
            ));
        }
    };

    if lines_val.is_empty() {
        return Err(AppError::BadRequest(
            "Journal lines cannot be empty".to_string(),
            "INVALID_JOURNAL_LINES",
        ));
    }

    let mut lines = Vec::with_capacity(lines_val.len());
    for item in lines_val {
        if !item.is_object() {
            return Err(AppError::BadRequest(
                "Each journal line must be a JSON object".to_string(),
                "INVALID_JOURNAL_LINES",
            ));
        }

        let account_code = item
            .get("account_code")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        let debit_val = item.get("debit").unwrap_or(&Value::Null);
        let credit_val = item.get("credit").unwrap_or(&Value::Null);

        // Strict integer Rupiah invariant: reject decimal floats, string amounts, booleans, negatives
        if debit_val.is_boolean()
            || credit_val.is_boolean()
            || debit_val.is_string()
            || credit_val.is_string()
            || (debit_val.is_f64() && !debit_val.is_i64())
            || (credit_val.is_f64() && !credit_val.is_i64())
        {
            return Err(AppError::BadRequest(
                "Journal amounts must be non-negative integers".to_string(),
                "INVALID_AMOUNT",
            ));
        }

        let debit = match debit_val {
            Value::Number(n) => n.as_i64().ok_or_else(|| {
                AppError::BadRequest(
                    "Journal amounts must be non-negative integers".to_string(),
                    "INVALID_AMOUNT",
                )
            })?,
            Value::Null => 0,
            _ => {
                return Err(AppError::BadRequest(
                    "Journal amounts must be non-negative integers".to_string(),
                    "INVALID_AMOUNT",
                ));
            }
        };

        let credit = match credit_val {
            Value::Number(n) => n.as_i64().ok_or_else(|| {
                AppError::BadRequest(
                    "Journal amounts must be non-negative integers".to_string(),
                    "INVALID_AMOUNT",
                )
            })?,
            Value::Null => 0,
            _ => {
                return Err(AppError::BadRequest(
                    "Journal amounts must be non-negative integers".to_string(),
                    "INVALID_AMOUNT",
                ));
            }
        };

        if debit < 0 || credit < 0 {
            return Err(AppError::BadRequest(
                "Journal amounts must be non-negative integers".to_string(),
                "INVALID_AMOUNT",
            ));
        }

        let memo = item
            .get("memo")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        lines.push(PostJournalLineRequest {
            account_code,
            debit,
            credit,
            memo,
        });
    }

    let entry_date = json_val
        .get("entry_date")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let description = json_val
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let source_type = json_val
        .get("source_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let source_id = json_val
        .get("source_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let req = PostJournalRequest {
        entry_date,
        description,
        source_type,
        source_id,
        lines,
    };

    let svc = state.accounting_service();
    let created = svc.post_journal(&ctx, req).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// GET /api/v1/accounting/journals/{id}
pub async fn get_journal_handler(
    ctx: TenantContext,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JournalEntryWithLines>, AppError> {
    let svc = state.accounting_service();
    let journal = svc.get_journal(&ctx, &id).await?;
    Ok(Json(journal))
}

/// PUT/DELETE/PATCH /api/v1/accounting/journals/{id}
/// Strictly rejected with HTTP 405 Method Not Allowed (PRD §11.2:577-586)
pub async fn immutable_journal_handler() -> Result<(), AppError> {
    Err(AppError::MethodNotAllowed(
        "Posted journals are immutable and cannot be updated directly; use reversal instead".to_string(),
        "JOURNAL_IMMUTABLE",
    ))
}

/// POST /api/v1/accounting/journals/{id}/reverse
pub async fn reverse_journal_handler(
    ctx: TenantContext,
    Path(id): Path<String>,
    State(state): State<AppState>,
    body: Option<Json<ReverseJournalRequest>>,
) -> Result<(StatusCode, Json<JournalEntryWithLines>), AppError> {
    let req = body.map(|b| b.0).unwrap_or(ReverseJournalRequest { reason: None });
    let svc = state.accounting_service();
    let reversed = svc.reverse_journal(&ctx, &id, req).await?;
    Ok((StatusCode::CREATED, Json(reversed)))
}

/// GET /api/v1/accounting/trial-balance
pub async fn trial_balance_handler(
    ctx: TenantContext,
    State(state): State<AppState>,
) -> Result<Json<TrialBalanceDto>, AppError> {
    let svc = state.accounting_service();
    let tb = svc.get_trial_balance(&ctx).await?;
    Ok(Json(tb))
}

/// POST /api/v1/accounting/tax/calculate
pub async fn calculate_tax_handler(
    body: Bytes,
) -> Result<Json<TaxCalculationResult>, AppError> {
    let json_val: Value = serde_json::from_slice(&body).map_err(|_| {
        AppError::BadRequest(
            "Amount must be an integer Rupiah".to_string(),
            "INVALID_AMOUNT",
        )
    })?;

    if !json_val.is_object() {
        return Err(AppError::BadRequest(
            "Amount must be an integer Rupiah".to_string(),
            "INVALID_AMOUNT",
        ));
    }

    let amount_val = json_val.get("amount").ok_or_else(|| {
        AppError::BadRequest(
            "Amount must be an integer Rupiah".to_string(),
            "INVALID_AMOUNT",
        )
    })?;

    // Must be integer, not string, not float, not boolean
    if amount_val.is_boolean() || amount_val.is_string() || (amount_val.is_f64() && !amount_val.is_i64()) {
        return Err(AppError::BadRequest(
            "Amount must be an integer Rupiah".to_string(),
            "INVALID_AMOUNT",
        ));
    }

    let amount = amount_val.as_i64().ok_or_else(|| {
        AppError::BadRequest(
            "Amount must be an integer Rupiah".to_string(),
            "INVALID_AMOUNT",
        )
    })?;

    if amount < 0 {
        return Err(AppError::BadRequest(
            "Amount cannot be negative".to_string(),
            "INVALID_AMOUNT",
        ));
    }

    let tax_type = json_val
        .get("tax_type")
        .and_then(|v| v.as_str())
        .unwrap_or("PPN_11_EXCL");

    let is_inclusive = json_val
        .get("is_inclusive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let result = calculate_tax_integer(amount, tax_type, is_inclusive)?;
    Ok(Json(result))
}

pub fn accounting_router() -> Router<AppState> {
    Router::new()
        .route("/accounts", get(list_accounts_handler).post(create_account_handler))
        .route("/accounts/{code}", axum::routing::delete(delete_account_handler))
        .route("/journals", get(list_journals_handler).post(post_journal_handler))
        .route(
            "/journals/{id}",
            get(get_journal_handler)
                .put(immutable_journal_handler)
                .delete(immutable_journal_handler)
                .patch(immutable_journal_handler),
        )
        .route("/journals/{id}/reverse", post(reverse_journal_handler))
        .route("/trial-balance", get(trial_balance_handler))
        .route("/tax/calculate", post(calculate_tax_handler))
}
