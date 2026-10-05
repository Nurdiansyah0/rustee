use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::money::Rupiah;
use crate::error::AppError;

/// Account classification according to standard financial accounting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Asset,
    Liability,
    Equity,
    Income,
    Expense,
}

impl AccountType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountType::Asset => "asset",
            AccountType::Liability => "liability",
            AccountType::Equity => "equity",
            AccountType::Income => "income",
            AccountType::Expense => "expense",
        }
    }

    pub fn from_str_case_insensitive(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "asset" => Some(AccountType::Asset),
            "liability" => Some(AccountType::Liability),
            "equity" => Some(AccountType::Equity),
            "income" => Some(AccountType::Income),
            "expense" => Some(AccountType::Expense),
            _ => None,
        }
    }
}

/// Chart of Accounts domain model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub tenant_id: String,
    pub code: String,
    pub name: String,
    pub account_type: AccountType,
    pub is_system: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl Account {
    pub const SYSTEM_CODES: [&'static str; 9] = [
        "1000", "1100", "1200", "1300", "2000", "2100", "4000", "5000", "6000",
    ];

    pub fn is_system_code(code: &str) -> bool {
        Self::SYSTEM_CODES.contains(&code.trim())
    }

    pub fn canonical_system_accounts() -> [(&'static str, &'static str, AccountType); 9] {
        [
            ("1000", "Kas", AccountType::Asset),
            ("1100", "Bank", AccountType::Asset),
            ("1200", "Piutang Usaha", AccountType::Asset),
            ("1300", "Persediaan Barang Dagang", AccountType::Asset),
            ("2000", "Utang Usaha", AccountType::Liability),
            ("2100", "Utang Pajak (PPN/PPh)", AccountType::Liability),
            ("4000", "Pendapatan Usaha", AccountType::Income),
            ("5000", "Beban Pokok Penjualan", AccountType::Expense),
            ("6000", "Beban Operasional", AccountType::Expense),
        ]
    }
}

/// Journal entry lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalStatus {
    Draft,
    Posted,
    Archived,
}

impl JournalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JournalStatus::Draft => "DRAFT",
            JournalStatus::Posted => "POSTED",
            JournalStatus::Archived => "ARCHIVED",
        }
    }
}

/// Journal entry source type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JournalSourceType {
    Manual,
    Invoice,
    Payment,
    Reversal,
    System,
    #[serde(rename = "PURCHASE_ORDER_RECEIPT")]
    PurchaseOrderReceipt,
    #[serde(rename = "INVENTORY_INBOUND")]
    InventoryInbound,
    #[serde(rename = "INVENTORY_OUTBOUND")]
    InventoryOutbound,
    #[serde(rename = "STOCK_ADJUSTMENT")]
    StockAdjustment,
}

impl JournalSourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            JournalSourceType::Manual => "MANUAL",
            JournalSourceType::Invoice => "INVOICE",
            JournalSourceType::Payment => "PAYMENT",
            JournalSourceType::Reversal => "REVERSAL",
            JournalSourceType::System => "SYSTEM",
            JournalSourceType::PurchaseOrderReceipt => "PURCHASE_ORDER_RECEIPT",
            JournalSourceType::InventoryInbound => "INVENTORY_INBOUND",
            JournalSourceType::InventoryOutbound => "INVENTORY_OUTBOUND",
            JournalSourceType::StockAdjustment => "STOCK_ADJUSTMENT",
        }
    }
}


/// Single journal line item
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalLine {
    pub id: String,
    pub journal_id: String,
    pub tenant_id: String,
    pub account_code: String,
    pub debit: Rupiah,
    pub credit: Rupiah,
    pub memo: Option<String>,
}

/// Header journal entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalEntry {
    pub id: String,
    pub tenant_id: String,
    pub entry_number: String,
    pub entry_date: String,
    pub description: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub status: String,
    pub is_reversed: i64,
    pub reversal_entry_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Journal line DTO returned in API responses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalLineDto {
    pub id: String,
    pub account_code: String,
    pub debit: i64,
    pub credit: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// Full journal entry with all line items and calculated totals
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalEntryWithLines {
    pub id: String,
    pub tenant_id: String,
    pub entry_number: String,
    pub entry_date: String,
    pub description: String,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub status: String,
    pub is_reversed: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversal_entry_id: Option<String>,
    pub total_debit: i64,
    pub total_credit: i64,
    pub lines: Vec<JournalLineDto>,
    pub created_at: String,
}

/// Inter-milestone posting command contract (PROJECT.md lines 89-106)
#[derive(Debug, Clone)]
pub struct PostJournalEntryCommand {
    pub tenant_id: Uuid,
    pub entry_date: DateTime<Utc>,
    pub description: String,
    pub source_type: String,
    pub source_id: Option<Uuid>,
    pub lines: Vec<PostJournalLineCommand>,
}

#[derive(Debug, Clone)]
pub struct PostJournalLineCommand {
    pub account_code: String,
    pub debit: Rupiah,
    pub credit: Rupiah,
    pub memo: Option<String>,
}

/// Supported tax types for Indonesian tax engine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaxType {
    Ppn11Excl,
    Ppn11Incl,
    Ppn12Excl,
    Ppn12Incl,
    Umkm05,
    Exempt,
}

impl TaxType {
    pub fn parse(s: &str, is_inclusive: bool) -> Result<Self, AppError> {
        match s.trim().to_uppercase().as_str() {
            "PPN_11_EXCL" => Ok(TaxType::Ppn11Excl),
            "PPN_11_INCL" => Ok(TaxType::Ppn11Incl),
            "PPN_11" => {
                if is_inclusive {
                    Ok(TaxType::Ppn11Incl)
                } else {
                    Ok(TaxType::Ppn11Excl)
                }
            }
            "PPN_12_EXCL" => Ok(TaxType::Ppn12Excl),
            "PPN_12_INCL" => Ok(TaxType::Ppn12Incl),
            "PPN_12" => {
                if is_inclusive {
                    Ok(TaxType::Ppn12Incl)
                } else {
                    Ok(TaxType::Ppn12Excl)
                }
            }
            "UMKM_05" | "UMKM_FINAL" => Ok(TaxType::Umkm05),
            "EXEMPT" | "NONE" => Ok(TaxType::Exempt),
            _ => Err(AppError::BadRequest(
                format!("Unsupported tax type '{}'", s),
                "INVALID_TAX_TYPE",
            )),
        }
    }
}

/// Deterministic pure integer tax calculation result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxCalculationResult {
    pub base_amount: i64,
    pub tax_type: String,
    pub is_inclusive: bool,
    pub tax_amount: i64,
    pub net_amount: i64,
    pub gross_amount: i64,
}

/// Pure integer division with deterministic half-up rounding:
/// round_half_up(N, D) = (N + floor(D / 2)) / D
/// All calculations operate on non-negative checked i128 to prevent overflow up to quadrillions.
pub fn round_half_up_i128(n: i128, d: i128) -> i64 {
    if d == 0 {
        return 0;
    }
    let rounded = (n + (d / 2)) / d;
    rounded as i64
}

/// Computes Indonesian taxes with zero float math and verified golden vectors
pub fn calculate_tax_integer(
    amount: i64,
    tax_type_str: &str,
    is_inclusive: bool,
) -> Result<TaxCalculationResult, AppError> {
    if amount < 0 {
        return Err(AppError::BadRequest(
            "Amount cannot be negative".to_string(),
            "INVALID_AMOUNT",
        ));
    }

    let parsed_type = TaxType::parse(tax_type_str, is_inclusive)?;
    let amt_128 = amount as i128;

    let (tax, net, gross) = match parsed_type {
        TaxType::Ppn11Excl => {
            // tax = round_half_up(amount * 11, 100)
            let tax = round_half_up_i128(amt_128 * 11, 100);
            let net = amount;
            let gross = net + tax;
            (tax, net, gross)
        }
        TaxType::Ppn11Incl => {
            // In exclusive pricing: gross = net + tax = net * 111 / 100
            // In inclusive extraction: tax = round_half_up(gross * 11, 111)
            let tax = round_half_up_i128(amt_128 * 11, 111);
            let gross = amount;
            let net = gross - tax; // Invariant: net + tax == gross strictly preserved
            (tax, net, gross)
        }
        TaxType::Ppn12Excl => {
            // tax = round_half_up(amount * 12, 100)
            let tax = round_half_up_i128(amt_128 * 12, 100);
            let net = amount;
            let gross = net + tax;
            (tax, net, gross)
        }
        TaxType::Ppn12Incl => {
            // tax = round_half_up(gross * 12, 112)
            let tax = round_half_up_i128(amt_128 * 12, 112);
            let gross = amount;
            let net = gross - tax; // Invariant: net + tax == gross strictly preserved
            (tax, net, gross)
        }
        TaxType::Umkm05 => {
            // Final 0.5% (50 bps) on gross turnover: round_half_up(turnover * 50, 10000)
            let tax = round_half_up_i128(amt_128 * 50, 10000);
            let base = amount;
            (tax, base, base)
        }
        TaxType::Exempt => (0, amount, amount),
    };

    Ok(TaxCalculationResult {
        base_amount: amount,
        tax_type: tax_type_str.to_string(),
        is_inclusive,
        tax_amount: tax,
        net_amount: net,
        gross_amount: gross,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_golden_tax_vectors() {
        // Vector 1: 100k PPN 11% Excl
        let v1 = calculate_tax_integer(100_000, "PPN_11_EXCL", false).unwrap();
        assert_eq!(v1.tax_amount, 11_000);
        assert_eq!(v1.net_amount, 100_000);
        assert_eq!(v1.gross_amount, 111_000);

        // Vector 2: 111k PPN 11% Incl
        let v2 = calculate_tax_integer(111_000, "PPN_11_INCL", true).unwrap();
        assert_eq!(v2.tax_amount, 11_000);
        assert_eq!(v2.net_amount, 100_000);
        assert_eq!(v2.gross_amount, 111_000);

        // Vector 3: 100k PPN 12% Excl
        let v3 = calculate_tax_integer(100_000, "PPN_12_EXCL", false).unwrap();
        assert_eq!(v3.tax_amount, 12_000);
        assert_eq!(v3.net_amount, 100_000);
        assert_eq!(v3.gross_amount, 112_000);

        // Vector 4: 112k PPN 12% Incl
        let v4 = calculate_tax_integer(112_000, "PPN_12_INCL", true).unwrap();
        assert_eq!(v4.tax_amount, 12_000);
        assert_eq!(v4.net_amount, 100_000);
        assert_eq!(v4.gross_amount, 112_000);

        // Vector 5: 105 PPN 11% Excl -> 11.55 rounds to 12
        let v5 = calculate_tax_integer(105, "PPN_11_EXCL", false).unwrap();
        assert_eq!(v5.tax_amount, 12);

        // Vector 6: 1 PPN 11% Excl -> 0.11 rounds to 0
        let v6 = calculate_tax_integer(1, "PPN_11_EXCL", false).unwrap();
        assert_eq!(v6.tax_amount, 0);

        // Vector 7: 5 PPN 11% Excl -> 0.55 rounds to 1
        let v7 = calculate_tax_integer(5, "PPN_11_EXCL", false).unwrap();
        assert_eq!(v7.tax_amount, 1);

        // Vector 8: 5 PPN 12% Excl -> 0.60 rounds to 1
        let v8 = calculate_tax_integer(5, "PPN_12_EXCL", false).unwrap();
        assert_eq!(v8.tax_amount, 1);

        // Vector 9: 0 PPN 11% Excl -> 0
        let v9 = calculate_tax_integer(0, "PPN_11_EXCL", false).unwrap();
        assert_eq!(v9.tax_amount, 0);

        // Vector 10: 1M UMKM 0.5% -> 5000
        let v10 = calculate_tax_integer(1_000_000, "UMKM_05", false).unwrap();
        assert_eq!(v10.tax_amount, 5_000);
        assert_eq!(v10.base_amount, 1_000_000);

        // Vector 12: 100 UMKM 0.5% -> 0.50 rounds to 1
        let v12 = calculate_tax_integer(100, "UMKM_05", false).unwrap();
        assert_eq!(v12.tax_amount, 1);

        // Vector 13: 99 UMKM 0.5% -> 0.495 rounds to 0
        let v13 = calculate_tax_integer(99, "UMKM_05", false).unwrap();
        assert_eq!(v13.tax_amount, 0);

        // Vector 16: 4.8B UMKM 0.5% -> 24M
        let v16 = calculate_tax_integer(4_800_000_000, "UMKM_05", false).unwrap();
        assert_eq!(v16.tax_amount, 24_000_000);

        // Vector 17: 9 Quadrillion IDR UMKM 0.5% -> 45 Trillion
        let v17 = calculate_tax_integer(9_000_000_000_000_000, "UMKM_05", false).unwrap();
        assert_eq!(v17.tax_amount, 45_000_000_000_000);

        // Vector 18: Exempt
        let v18 = calculate_tax_integer(250_000, "EXEMPT", false).unwrap();
        assert_eq!(v18.tax_amount, 0);
        assert_eq!(v18.net_amount, 250_000);
        assert_eq!(v18.gross_amount, 250_000);
    }
}
