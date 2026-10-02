use crate::domain::money::Rupiah;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

// ---------------------------------------------------------------------------
// Core Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    HIGH,
    MEDIUM,
    LOW,
}

impl fmt::Display for ConfidenceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfidenceLevel::HIGH => write!(f, "HIGH"),
            ConfidenceLevel::MEDIUM => write!(f, "MEDIUM"),
            ConfidenceLevel::LOW => write!(f, "LOW"),
        }
    }
}

impl std::str::FromStr for ConfidenceLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "HIGH" => Ok(ConfidenceLevel::HIGH),
            "MEDIUM" => Ok(ConfidenceLevel::MEDIUM),
            "LOW" => Ok(ConfidenceLevel::LOW),
            _ => Err(format!("Unknown confidence level: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionSource {
    Notification,
    Sms,
    Gmail,
    Manual,
}

impl fmt::Display for IngestionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestionSource::Notification => write!(f, "notification"),
            IngestionSource::Sms => write!(f, "sms"),
            IngestionSource::Gmail => write!(f, "gmail"),
            IngestionSource::Manual => write!(f, "manual"),
        }
    }
}

impl std::str::FromStr for IngestionSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "notification" => Ok(IngestionSource::Notification),
            "sms" => Ok(IngestionSource::Sms),
            "gmail" => Ok(IngestionSource::Gmail),
            "manual" => Ok(IngestionSource::Manual),
            _ => Err(format!("Unknown ingestion source: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionStatus {
    Pending,
    AutoCreated,
    RequiresConfirmation,
    Rejected,
    Duplicate,
}

impl fmt::Display for IngestionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestionStatus::Pending => write!(f, "pending"),
            IngestionStatus::AutoCreated => write!(f, "auto_created"),
            IngestionStatus::RequiresConfirmation => write!(f, "requires_confirmation"),
            IngestionStatus::Rejected => write!(f, "rejected"),
            IngestionStatus::Duplicate => write!(f, "duplicate"),
        }
    }
}

impl std::str::FromStr for IngestionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(IngestionStatus::Pending),
            "auto_created" => Ok(IngestionStatus::AutoCreated),
            "requires_confirmation" => Ok(IngestionStatus::RequiresConfirmation),
            "rejected" => Ok(IngestionStatus::Rejected),
            "duplicate" => Ok(IngestionStatus::Duplicate),
            _ => Err(format!("Unknown ingestion status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionDirection {
    Income,
    Expense,
}

impl fmt::Display for TransactionDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionDirection::Income => write!(f, "income"),
            TransactionDirection::Expense => write!(f, "expense"),
        }
    }
}

impl std::str::FromStr for TransactionDirection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "income" | "cr" | "kredit" | "masuk" => Ok(TransactionDirection::Income),
            "expense" | "db" | "debit" | "keluar" => Ok(TransactionDirection::Expense),
            _ => Err(format!("Unknown transaction direction: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical Transaction Candidate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalTransactionCandidate {
    pub candidate_id: String,
    pub event_id: String,
    pub transaction_id: Option<String>,
    pub amount: Rupiah,
    pub currency: String,
    pub direction: TransactionDirection,
    pub occurred_at: String, // ISO 8601 UTC
    pub provider: String,
    pub merchant: Option<String>,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub source: IngestionSource,
    pub external_reference: Option<String>,
    pub confidence: ConfidenceLevel,
    pub requires_confirmation: bool,
}

impl CanonicalTransactionCandidate {
    /// Checks cross-source deduplication matching:
    /// Matches signals: provider, external_reference, amount, direction, time window (±window_secs), and merchant.
    pub fn is_duplicate_of(&self, other: &CanonicalTransactionCandidate, window_secs: i64) -> bool {
        // 1. Amount and Direction must strictly match
        if self.amount != other.amount || self.direction != other.direction {
            return false;
        }

        // 2. Provider must match (case-insensitive)
        if !self.provider.eq_ignore_ascii_case(&other.provider) {
            return false;
        }

        // 3. Time window check (±window_secs, default 300s = 5 minutes)
        if !within_time_window(&self.occurred_at, &other.occurred_at, window_secs) {
            return false;
        }

        // 4. External reference check (if both exist and are non-empty)
        if let (Some(ref1), Some(ref2)) = (&self.external_reference, &other.external_reference) {
            if !ref1.is_empty() && !ref2.is_empty() {
                let matches =
                    ref1.eq_ignore_ascii_case(ref2) || ref1.contains(ref2) || ref2.contains(ref1);
                if !matches {
                    return false;
                }
                return true;
            }
        }

        // 5. Merchant check (if both exist and are non-empty)
        if let (Some(m1), Some(m2)) = (&self.merchant, &other.merchant) {
            let n1 = normalize_merchant_name(m1);
            let n2 = normalize_merchant_name(m2);
            if !n1.is_empty() && !n2.is_empty() {
                if !(n1 == n2 || n1.contains(&n2) || n2.contains(&n1)) {
                    return false;
                }
                return true;
            }
        }

        // 6. Cross-source fallback: if amount, direction, provider, and time window (±300s) match
        // and there is no conflicting merchant or reference, treat as duplicate.
        true
    }
}

// ---------------------------------------------------------------------------
// Privacy & Payload Minimization (REQ-INGEST-08)
// ---------------------------------------------------------------------------

/// Computes a deterministic SHA-256 hex digest of the raw payload.
/// Master Specification v3.1.0 REQ-INGEST-08 strictly mandates that raw notification, SMS,
/// and email payloads are discarded immediately after field extraction, persisting only this hash.
pub fn compute_payload_hash(raw_payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_payload.as_bytes());
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Parsed Intermediate Signal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSignal {
    pub provider: String,
    pub amount: Option<Rupiah>,
    pub direction: Option<TransactionDirection>,
    pub merchant: Option<String>,
    pub external_reference: Option<String>,
    pub occurred_at: Option<String>,
    pub is_financial: bool,
}

// ---------------------------------------------------------------------------
// Parsing and Normalization Helpers
// ---------------------------------------------------------------------------

/// Parse an Indonesian Rupiah amount from unstructured text.
/// Handles formats:
/// - "Rp 50.000"
/// - "Rp50.000"
/// - "Rp 50.000,00"
/// - "Rp. 1.500.000"
/// - "IDR 75,000.00"
/// - "sebesar Rp 200.000"
/// - "DB 50.000,00"
/// - "CR 100.000,00"
pub fn parse_idr_amount(text: &str) -> Option<Rupiah> {
    let lower = text.to_lowercase();

    // Look for currency prefixes: "rp", "rp.", "idr", "idr.", "sebesar", "db", "cr"
    let prefixes = [
        "rp.",
        "rp",
        "idr.",
        "idr",
        "sebesar rp.",
        "sebesar rp",
        "sebesar",
        "db",
        "cr",
    ];

    for prefix in &prefixes {
        if let Some(pos) = lower.find(prefix) {
            let start = pos + prefix.len();
            let remainder = &lower[start..];

            // Extract digits, dots, commas, spaces
            let mut num_str = String::new();
            let mut found_digit = false;

            for ch in remainder.chars() {
                if ch.is_ascii_digit() {
                    num_str.push(ch);
                    found_digit = true;
                } else if ch == '.' || ch == ',' {
                    if found_digit {
                        num_str.push(ch);
                    }
                } else if found_digit {
                    break;
                }
            }

            if let Some(cleaned) = clean_numeric_amount(&num_str) {
                if cleaned > 0 {
                    return Some(Rupiah::new(cleaned));
                }
            }
        }
    }

    // Secondary fallback: find any sequence of digits with standard Indonesian thousands dot e.g. "50.000"
    for word in text.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != ',');
        if let Some(val) = clean_numeric_amount(trimmed) {
            if val >= 1000 {
                return Some(Rupiah::new(val));
            }
        }
    }

    None
}

fn clean_numeric_amount(raw: &str) -> Option<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let last_comma = trimmed.rfind(',');
    let last_dot = trimmed.rfind('.');

    let without_decimal = match (last_comma, last_dot) {
        (Some(c_idx), Some(d_idx)) => {
            if d_idx > c_idx {
                // e.g. 75,000.00 -> dot is decimal
                let dec = trimmed[d_idx + 1..].trim();
                if (dec.len() == 1 || dec.len() == 2) && dec.chars().all(|c| c.is_ascii_digit()) {
                    &trimmed[..d_idx]
                } else {
                    trimmed
                }
            } else {
                // e.g. 75.000,00 -> comma is decimal
                let dec = trimmed[c_idx + 1..].trim();
                if (dec.len() == 1 || dec.len() == 2) && dec.chars().all(|c| c.is_ascii_digit()) {
                    &trimmed[..c_idx]
                } else {
                    trimmed
                }
            }
        }
        (Some(c_idx), None) => {
            let dec = trimmed[c_idx + 1..].trim();
            if (dec.len() == 1 || dec.len() == 2) && dec.chars().all(|c| c.is_ascii_digit()) {
                &trimmed[..c_idx]
            } else {
                trimmed
            }
        }
        (None, Some(d_idx)) => {
            let dec = trimmed[d_idx + 1..].trim();
            if (dec.len() == 1 || dec.len() == 2) && dec.chars().all(|c| c.is_ascii_digit()) {
                &trimmed[..d_idx]
            } else {
                trimmed
            }
        }
        (None, None) => trimmed,
    };

    // Filter to only digits
    let digits: String = without_decimal
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<i64>().ok()
}

pub fn normalize_merchant_name(name: &str) -> String {
    let mut cleaned = name
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();

    // Remove common prefixes
    for p in &["pt ", "cv ", "toko ", "warung ", "resto ", "merchant "] {
        if cleaned.starts_with(p) {
            cleaned = cleaned[p.len()..].to_string();
        }
    }

    cleaned.trim().to_string()
}

pub fn within_time_window(t1_str: &str, t2_str: &str, window_secs: i64) -> bool {
    let parse1 = DateTime::parse_from_rfc3339(t1_str).map(|d| d.with_timezone(&Utc));
    let parse2 = DateTime::parse_from_rfc3339(t2_str).map(|d| d.with_timezone(&Utc));

    match (parse1, parse2) {
        (Ok(dt1), Ok(dt2)) => {
            let diff = (dt1.timestamp() - dt2.timestamp()).abs();
            diff <= window_secs
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Provider Detection & Specialized Parsers
// ---------------------------------------------------------------------------

pub fn detect_provider(package_or_source: &str, content: &str) -> String {
    let lower_pkg = package_or_source.to_lowercase();
    let lower_content = content.to_lowercase();

    if lower_pkg.contains("bca")
        || lower_content.contains("bca")
        || lower_content.contains("klikbca")
    {
        "bca".to_string()
    } else if lower_pkg.contains("mandiri")
        || lower_content.contains("mandiri")
        || lower_content.contains("livin")
    {
        "mandiri".to_string()
    } else if lower_pkg.contains("bri")
        || lower_content.contains("brimo")
        || lower_content.contains("bank bri")
    {
        "bri".to_string()
    } else if lower_pkg.contains("bni")
        || lower_content.contains("wondr")
        || lower_content.contains("bni mobile")
    {
        "bni".to_string()
    } else if lower_pkg.contains("dana") || lower_content.contains("dana") {
        "dana".to_string()
    } else if lower_pkg.contains("gojek")
        || lower_pkg.contains("gopay")
        || lower_content.contains("gopay")
    {
        "gopay".to_string()
    } else if lower_pkg.contains("ovo") || lower_content.contains("ovo") {
        "ovo".to_string()
    } else if lower_pkg.contains("shopee") || lower_content.contains("shopeepay") {
        "shopeepay".to_string()
    } else {
        "unknown".to_string()
    }
}

pub fn parse_external_reference(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let markers = [
        "ref no:",
        "ref no",
        "no. ref:",
        "no. ref",
        "no ref:",
        "no ref",
        "ref:",
        "ref",
        "no. transaksi:",
        "no transaksi:",
        "no. transaksi",
        "order id:",
        "rrn:",
        "stan:",
    ];

    for marker in &markers {
        if let Some(pos) = lower.find(marker) {
            let remainder = text[pos + marker.len()..].trim();
            // Take the first alphanumeric token
            let token: String = remainder
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !token.is_empty() {
                return Some(token);
            }
        }
    }

    None
}

pub fn parse_merchant_candidate(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    // Look for phrases like "di <merchant>", "ke <merchant>", "pada <merchant>", "untuk <merchant>"
    let markers = [" di ", " ke ", " pada ", " untuk "];

    for marker in &markers {
        if let Some(pos) = lower.find(marker) {
            let remainder = text[pos + marker.len()..].trim();
            // Cut off at common trailing phrases like "berhasil", "sukses", "sebesar", "pada", "ref"
            let mut words = Vec::new();
            for word in remainder.split_whitespace() {
                let w_lower = word.to_lowercase();
                if w_lower.starts_with("berhasil")
                    || w_lower.starts_with("sukses")
                    || w_lower.starts_with("sebesar")
                    || w_lower.starts_with("rp")
                    || w_lower.starts_with("pada")
                    || w_lower.starts_with("ref")
                    || w_lower.starts_with("no")
                {
                    break;
                }
                words.push(word);
                if words.len() >= 4 {
                    break;
                }
            }

            if !words.is_empty() {
                let candidate = words
                    .join(" ")
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string();
                if !candidate.is_empty() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Indonesian Provider Notification Parsers
// ---------------------------------------------------------------------------

pub fn parse_bca_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("transfer")
        || lower.contains("pembayaran")
        || lower.contains("qris")
        || lower.contains("trsf")
        || lower.contains("debit")
        || lower.contains("kredit")
        || lower.contains("cr")
        || lower.contains("db");

    let amount = parse_idr_amount(&combined);

    let direction = if lower.contains("cr")
        || lower.contains("transfer dari")
        || lower.contains("dana masuk")
        || lower.contains("menerima")
    {
        Some(TransactionDirection::Income)
    } else if lower.contains("db")
        || lower.contains("transfer ke")
        || lower.contains("pembayaran")
        || lower.contains("qris")
        || lower.contains("bayar")
        || lower.contains("tarik tunai")
        || (lower.contains("transfer") && lower.contains("ke "))
        || lower.contains("m-transfer")
    {
        Some(TransactionDirection::Expense)
    } else {
        None
    };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "bca".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_mandiri_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("transfer")
        || lower.contains("pembayaran")
        || lower.contains("qris")
        || lower.contains("dana masuk")
        || lower.contains("debit")
        || lower.contains("kredit")
        || lower.contains("kirim");

    let amount = parse_idr_amount(&combined);

    let direction = if lower.contains("dana masuk")
        || lower.contains("kredit rekening")
        || lower.contains("menerima")
    {
        Some(TransactionDirection::Income)
    } else if lower.contains("mengirim")
        || lower.contains("pembayaran")
        || lower.contains("qris")
        || lower.contains("debit rekening")
        || lower.contains("transfer berhasil")
    {
        Some(TransactionDirection::Expense)
    } else {
        None
    };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "mandiri".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_bri_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("transaksi berhasil")
        || lower.contains("transfer")
        || lower.contains("qris")
        || lower.contains("pembayaran")
        || lower.contains("dikredit")
        || lower.contains("didebet")
        || lower.contains("transaksi masuk");

    let amount = parse_idr_amount(&combined);

    let direction = if lower.contains("transaksi masuk")
        || lower.contains("dikredit")
        || lower.contains("terima")
    {
        Some(TransactionDirection::Income)
    } else {
        Some(TransactionDirection::Expense)
    };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "bri".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_bni_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("transaksi sukses")
        || lower.contains("transfer")
        || lower.contains("pembayaran")
        || lower.contains("qris")
        || lower.contains("kredit")
        || lower.contains("debit");

    let amount = parse_idr_amount(&combined);

    let direction = if lower.contains("kredit") || lower.contains("dana masuk") {
        Some(TransactionDirection::Income)
    } else {
        Some(TransactionDirection::Expense)
    };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "bni".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_dana_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("pembayaran")
        || lower.contains("kirim uang")
        || lower.contains("menerima")
        || lower.contains("isi saldo")
        || lower.contains("top up")
        || lower.contains("berhasil");

    let amount = parse_idr_amount(&combined);

    let direction =
        if lower.contains("menerima") || lower.contains("isi saldo") || lower.contains("bertambah")
        {
            Some(TransactionDirection::Income)
        } else {
            Some(TransactionDirection::Expense)
        };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "dana".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_gopay_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("bayar")
        || lower.contains("transfer")
        || lower.contains("top up")
        || lower.contains("transferan")
        || lower.contains("berhasil");

    let amount = parse_idr_amount(&combined);

    let direction = if lower.contains("top up")
        || lower.contains("transferan")
        || lower.contains("bertambah")
        || lower.contains("menerima")
    {
        Some(TransactionDirection::Income)
    } else {
        Some(TransactionDirection::Expense)
    };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "gopay".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_ovo_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("pembayaran")
        || lower.contains("transfer")
        || lower.contains("top up")
        || lower.contains("bertambah")
        || lower.contains("berhasil");

    let amount = parse_idr_amount(&combined);

    let direction = if lower.contains("top up") || lower.contains("bertambah") {
        Some(TransactionDirection::Income)
    } else {
        Some(TransactionDirection::Expense)
    };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "ovo".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_shopeepay_notification(title: &str, text: &str) -> ParsedSignal {
    let combined = format!("{} {}", title, text);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("pembayaran")
        || lower.contains("transfer")
        || lower.contains("top up")
        || lower.contains("isi saldo")
        || lower.contains("menerima")
        || lower.contains("berhasil");

    let amount = parse_idr_amount(&combined);

    let direction =
        if lower.contains("top up") || lower.contains("isi saldo") || lower.contains("menerima") {
            Some(TransactionDirection::Income)
        } else {
            Some(TransactionDirection::Expense)
        };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider: "shopeepay".to_string(),
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

// ---------------------------------------------------------------------------
// SMS and Gmail Parsers
// ---------------------------------------------------------------------------

pub fn parse_sms_payload(sender: &str, body: &str) -> ParsedSignal {
    let provider = detect_provider(sender, body);
    let combined = format!("{} {}", sender, body);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("trsf")
        || lower.contains("transfer")
        || lower.contains("debit")
        || lower.contains("kredit")
        || lower.contains("db ")
        || lower.contains("cr ")
        || lower.contains("didebet")
        || lower.contains("dikredit")
        || lower.contains("pembayaran")
        || lower.contains("berhasil");

    let amount = parse_idr_amount(body);

    let direction = if lower.contains(" cr ")
        || lower.contains("kredit")
        || lower.contains("dikredit")
        || lower.contains("dana masuk")
    {
        Some(TransactionDirection::Income)
    } else if lower.contains(" db ")
        || lower.contains("debit")
        || lower.contains("didebet")
        || lower.contains("transfer")
        || lower.contains("pembayaran")
    {
        Some(TransactionDirection::Expense)
    } else {
        None
    };

    let merchant = parse_merchant_candidate(body);
    let external_reference = parse_external_reference(body);

    ParsedSignal {
        provider,
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

pub fn parse_gmail_payload(snippet: &str, body: &str) -> ParsedSignal {
    let combined = format!("{} {}", snippet, body);
    let provider = detect_provider("", &combined);
    let lower = combined.to_lowercase();

    let is_financial = lower.contains("bukti pembayaran")
        || lower.contains("tanda terima")
        || lower.contains("invoice")
        || lower.contains("receipt")
        || lower.contains("pesanan")
        || lower.contains("berhasil");

    let amount = parse_idr_amount(&combined);
    let direction = if lower.contains("pengembalian dana") || lower.contains("refund") {
        Some(TransactionDirection::Income)
    } else {
        Some(TransactionDirection::Expense)
    };

    let merchant = parse_merchant_candidate(&combined);
    let external_reference = parse_external_reference(&combined);

    ParsedSignal {
        provider,
        amount,
        direction,
        merchant,
        external_reference,
        occurred_at: None,
        is_financial,
    }
}

// ---------------------------------------------------------------------------
// Confidence Engine
// ---------------------------------------------------------------------------

/// Master Specification v3.1.0 §24 Confidence Engine:
/// - HIGH: Recognized provider pattern and complete information. Auto-creates transaction after domain validation.
/// - MEDIUM: Ambiguity in account/category or partial information. Stored as candidate requiring user confirmation.
/// - LOW: Unresolved or invalid. Rejected.
pub fn evaluate_confidence(
    signal: &ParsedSignal,
    account_resolved: bool,
) -> (ConfidenceLevel, bool) {
    // If not a financial notification or amount is missing/0 -> LOW
    let amount_valid = signal.amount.map(|a| a.is_positive()).unwrap_or(false);
    if !signal.is_financial || !amount_valid {
        return (ConfidenceLevel::LOW, false);
    }

    // Direction must be known
    if signal.direction.is_none() {
        return (ConfidenceLevel::LOW, false);
    }

    // Provider check
    let provider_recognized = signal.provider != "unknown";

    if provider_recognized && account_resolved {
        // High confidence: complete information and known account
        (ConfidenceLevel::HIGH, false)
    } else if provider_recognized || amount_valid {
        // Medium confidence: valid financial transaction, but account or category needs confirmation
        (ConfidenceLevel::MEDIUM, true)
    } else {
        (ConfidenceLevel::LOW, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_amount_parsing_various_indonesian_formats() {
        assert_eq!(
            parse_idr_amount("Transfer Rp 50.000 ke 1234567890"),
            Some(Rupiah::new(50000))
        );
        assert_eq!(
            parse_idr_amount("Pembayaran Rp50.000,00 berhasil"),
            Some(Rupiah::new(50000))
        );
        assert_eq!(
            parse_idr_amount("Total sebesar Rp. 1.250.000 berhasil"),
            Some(Rupiah::new(1250000))
        );
        assert_eq!(parse_idr_amount("IDR 75,000.00"), Some(Rupiah::new(75000)));
        assert_eq!(
            parse_idr_amount("TRSF E-BANKING DB 100.000,00"),
            Some(Rupiah::new(100000))
        );
        assert_eq!(parse_idr_amount("Promo diskon 50% tanpa nominal"), None);
    }

    #[test]
    fn test_bca_notification_parser() {
        let signal = parse_bca_notification(
            "m-Transfer Berhasil",
            "Transfer Rp 50.000 ke 1234567890 BERHASIL. Ref: 987654",
        );
        assert_eq!(signal.provider, "bca");
        assert_eq!(signal.amount, Some(Rupiah::new(50000)));
        assert_eq!(signal.direction, Some(TransactionDirection::Expense));
        assert_eq!(signal.external_reference, Some("987654".to_string()));
        assert!(signal.is_financial);
    }

    #[test]
    fn test_mandiri_income_parser() {
        let signal = parse_mandiri_notification(
            "Livin' by Mandiri",
            "Dana Masuk: Rekening Anda menerima Rp 500.000 dari Ahmad",
        );
        assert_eq!(signal.provider, "mandiri");
        assert_eq!(signal.amount, Some(Rupiah::new(500000)));
        assert_eq!(signal.direction, Some(TransactionDirection::Income));
    }

    #[test]
    fn test_cross_source_deduplication() {
        let now = Utc::now().to_rfc3339();
        let c1 = CanonicalTransactionCandidate {
            candidate_id: "c1".to_string(),
            event_id: "e1".to_string(),
            transaction_id: None,
            amount: Rupiah::new(50000),
            currency: "IDR".to_string(),
            direction: TransactionDirection::Expense,
            occurred_at: now.clone(),
            provider: "bca".to_string(),
            merchant: Some("Kopi Kenangan".to_string()),
            account_id: Some("acc1".to_string()),
            category_id: None,
            source: IngestionSource::Notification,
            external_reference: Some("REF123".to_string()),
            confidence: ConfidenceLevel::HIGH,
            requires_confirmation: false,
        };

        let mut c2 = c1.clone();
        c2.candidate_id = "c2".to_string();
        c2.source = IngestionSource::Sms;

        assert!(c1.is_duplicate_of(&c2, 300));
    }

    #[test]
    fn test_privacy_payload_minimization() {
        let raw = "Transfer Rp 50.000 ke 1234567890 BERHASIL rahasia nomor pin 1234";
        let hash = compute_payload_hash(raw);
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, compute_payload_hash(raw));
    }

    #[test]
    fn test_cross_source_deduplication_different_merchants_not_duplicate() {
        let now = Utc::now().to_rfc3339();
        let c1 = CanonicalTransactionCandidate {
            candidate_id: "c1".to_string(),
            event_id: "e1".to_string(),
            transaction_id: None,
            amount: Rupiah::new(50000),
            currency: "IDR".to_string(),
            direction: TransactionDirection::Expense,
            occurred_at: now.clone(),
            provider: "bca".to_string(),
            merchant: Some("Kopi Kenangan".to_string()),
            account_id: Some("acc1".to_string()),
            category_id: None,
            source: IngestionSource::Notification,
            external_reference: None,
            confidence: ConfidenceLevel::HIGH,
            requires_confirmation: false,
        };

        let mut c2 = c1.clone();
        c2.candidate_id = "c2".to_string();
        c2.merchant = Some("Indomaret".to_string());
        c2.source = IngestionSource::Sms;

        assert!(!c1.is_duplicate_of(&c2, 300));
    }

    #[test]
    fn test_cross_source_deduplication_different_references_not_duplicate() {
        let now = Utc::now().to_rfc3339();
        let c1 = CanonicalTransactionCandidate {
            candidate_id: "c1".to_string(),
            event_id: "e1".to_string(),
            transaction_id: None,
            amount: Rupiah::new(50000),
            currency: "IDR".to_string(),
            direction: TransactionDirection::Expense,
            occurred_at: now.clone(),
            provider: "bca".to_string(),
            merchant: Some("Kopi Kenangan".to_string()),
            account_id: Some("acc1".to_string()),
            category_id: None,
            source: IngestionSource::Notification,
            external_reference: Some("REF-1111".to_string()),
            confidence: ConfidenceLevel::HIGH,
            requires_confirmation: false,
        };

        let mut c2 = c1.clone();
        c2.candidate_id = "c2".to_string();
        c2.external_reference = Some("REF-9999".to_string());
        c2.source = IngestionSource::Sms;

        assert!(!c1.is_duplicate_of(&c2, 300));
    }
}
