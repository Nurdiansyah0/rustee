//! Inventory Domain Models & Valuation Engine (Features 1, 2, 3, 5, 6, 10, 13, 17)
//!
//! Provides domain models for multi-location warehouse tracking, products with sequential SKUs,
//! stock items with atomic negative balance prevention, immutable stock movement audit ledgers,
//! purchase order workflows, stock adjustments, and Indonesian Weighted Average Cost (WAC) arithmetic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::accounting::round_half_up_i128;
use crate::domain::money::Rupiah;
use crate::error::AppError;

/// Multi-location warehouse domain entity (R1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warehouse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
}

/// Catalog product domain entity (R1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub cost_price: Rupiah,
    pub sale_price: Rupiah,
    pub reorder_threshold: i64,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Multi-location stock balance domain entity (R1, R2)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItem {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub warehouse_id: Uuid,
    pub product_id: Uuid,
    pub quantity_on_hand: i64,
    pub quantity_reserved: i64,
    pub reorder_threshold: i64,
    pub bin_location: Option<String>,
    pub average_cost: Rupiah,
    pub updated_at: DateTime<Utc>,
}

impl StockItem {
    /// Available stock calculated as quantity on hand minus reserved stock
    #[inline]
    pub fn available_quantity(&self) -> i64 {
        self.quantity_on_hand.saturating_sub(self.quantity_reserved)
    }

    /// True if quantity on hand has reached or fallen below the reorder threshold
    #[inline]
    pub fn is_low_stock(&self) -> bool {
        self.quantity_on_hand <= self.reorder_threshold
    }
}

/// Stock movement mutation type enum (R2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StockMovementType {
    Inbound,
    Outbound,
    Transfer,
    Adjustment,
}

impl StockMovementType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbound => "INBOUND",
            Self::Outbound => "OUTBOUND",
            Self::Transfer => "TRANSFER",
            Self::Adjustment => "ADJUSTMENT",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "INBOUND" => Some(Self::Inbound),
            "OUTBOUND" => Some(Self::Outbound),
            "TRANSFER" => Some(Self::Transfer),
            "ADJUSTMENT" => Some(Self::Adjustment),
            _ => None,
        }
    }
}

/// Immutable stock movement ledger entry (R2)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockMovement {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub movement_type: StockMovementType,
    pub product_id: Uuid,
    pub source_warehouse_id: Option<Uuid>,
    pub destination_warehouse_id: Option<Uuid>,
    pub quantity: i64,
    pub unit_cost: Option<Rupiah>,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub batch_number: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub actor_id: Option<Uuid>,
}

/// Command to create a stock movement
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateStockMovementCommand {
    pub tenant_id: Uuid,
    pub movement_type: StockMovementType,
    pub product_id: Uuid,
    pub source_warehouse_id: Option<Uuid>,
    pub destination_warehouse_id: Option<Uuid>,
    pub quantity: i64,
    pub unit_cost: Option<Rupiah>,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub batch_number: Option<String>,
    pub notes: Option<String>,
}

/// Purchase Order lifecycle state machine (R3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseOrderStatus {
    Draft,
    Ordered,
    PartiallyReceived,
    Received,
    Cancelled,
}

impl PurchaseOrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Ordered => "ORDERED",
            Self::PartiallyReceived => "PARTIALLY_RECEIVED",
            Self::Received => "RECEIVED",
            Self::Cancelled => "CANCELLED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "DRAFT" => Some(Self::Draft),
            "ORDERED" => Some(Self::Ordered),
            "PARTIALLY_RECEIVED" => Some(Self::PartiallyReceived),
            "RECEIVED" => Some(Self::Received),
            "CANCELLED" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn can_modify_items(&self) -> bool {
        matches!(self, Self::Draft)
    }

    pub fn can_order(&self) -> bool {
        matches!(self, Self::Draft)
    }

    pub fn can_receive(&self) -> bool {
        matches!(self, Self::Ordered | Self::PartiallyReceived)
    }

    pub fn can_cancel(&self) -> bool {
        matches!(self, Self::Draft | Self::Ordered)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Received | Self::Cancelled)
    }
}

/// Purchase Order domain entity (R3)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub po_number: String,
    pub supplier_name: String,
    pub destination_warehouse_id: Uuid,
    pub status: PurchaseOrderStatus,
    pub total_amount: Rupiah,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Purchase Order Line Item domain entity (R3)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderItem {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub purchase_order_id: Uuid,
    pub product_id: Uuid,
    pub quantity_ordered: i64,
    pub quantity_received: i64,
    pub unit_cost: Rupiah,
    pub total_cost: Rupiah,
}

impl PurchaseOrderItem {
    pub fn remaining_quantity(&self) -> i64 {
        self.quantity_ordered.saturating_sub(self.quantity_received)
    }
}

/// Stock physical count adjustment domain entity (R2, R4)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockAdjustment {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub adjustment_number: String,
    pub warehouse_id: Uuid,
    pub product_id: Uuid,
    pub variance_quantity: i64,
    pub previous_quantity: i64,
    pub new_quantity: i64,
    pub reason: String,
    pub journal_entry_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub actor_id: Option<Uuid>,
}

/// Detailed stock item projection including joined product and warehouse metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItemWithDetails {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub warehouse_id: Uuid,
    pub product_id: Uuid,
    pub quantity_on_hand: i64,
    pub quantity_reserved: i64,
    pub reorder_threshold: i64,
    pub bin_location: Option<String>,
    pub average_cost: Rupiah,
    pub updated_at: DateTime<Utc>,
    pub product_name: String,
    pub product_sku: String,
    pub product_unit: String,
    pub warehouse_name: String,
    pub warehouse_code: String,
    pub is_low_stock: bool,
}

/// Request to create a new warehouse
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWarehouseRequest {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

/// Request to create a new product
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProductRequest {
    #[serde(default)]
    pub sku: Option<String>,
    pub name: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    #[serde(default)]
    pub cost_price: Rupiah,
    #[serde(default)]
    pub sale_price: Rupiah,
    #[serde(default)]
    pub reorder_threshold: i64,
}

fn default_unit() -> String {
    "pcs".to_string()
}

/// Request to record a direct stock movement (INBOUND or OUTBOUND)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateMovementRequest {
    pub movement_type: StockMovementType,
    pub product_id: Uuid,
    pub quantity: i64,
    #[serde(default)]
    pub source_warehouse_id: Option<Uuid>,
    #[serde(default)]
    pub destination_warehouse_id: Option<Uuid>,
    #[serde(default)]
    pub unit_cost: Option<Rupiah>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub batch_number: Option<String>,
}

/// Result of a stock movement mutation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockMovementResult {
    pub id: Uuid,
    pub movement_type: StockMovementType,
    pub product_id: Uuid,
    pub quantity: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_stock: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resulting_stock: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_cost: Option<Rupiah>,
    pub created_at: DateTime<Utc>,
}

/// Request to transfer stock between warehouses
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferStockRequest {
    pub source_warehouse_id: Uuid,
    pub destination_warehouse_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i64,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Result of an inter-warehouse stock transfer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockTransferResult {
    pub movement_id: Uuid,
    pub source_warehouse_id: Uuid,
    pub destination_warehouse_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i64,
    pub status: String,
    pub source_remaining: i64,
    pub destination_total: i64,
    pub created_at: DateTime<Utc>,
}

/// Request to adjust physical stock count
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjustStockRequest {
    pub warehouse_id: Uuid,
    pub product_id: Uuid,
    pub actual_quantity: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Result of a physical stock count adjustment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockAdjustmentResult {
    pub id: Uuid,
    pub adjustment_number: String,
    pub warehouse_id: Uuid,
    pub product_id: Uuid,
    pub previous_quantity: i64,
    pub actual_quantity: i64,
    pub new_quantity: i64,
    pub variance: i64,
    pub variance_quantity: i64,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_entry_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Wrapper response for warehouse list
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarehousesListResponse {
    pub warehouses: Vec<Warehouse>,
    pub count: usize,
}

/// Wrapper response for product list
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductsListResponse {
    pub products: Vec<Product>,
    pub count: usize,
}

/// Wrapper response for stock items list
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItemsListResponse {
    pub stock_items: Vec<StockItemWithDetails>,
    pub count: usize,
}

/// Wrapper response for stock movements audit list
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockMovementsListResponse {
    pub movements: Vec<StockMovement>,
    pub count: usize,
}

/// Request to create a purchase order item
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePurchaseOrderItemRequest {
    pub product_id: Uuid,
    pub quantity_ordered: i64,
    pub unit_cost: Rupiah,
}

/// Request to create a purchase order
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePurchaseOrderRequest {
    pub supplier_name: String,
    pub destination_warehouse_id: Uuid,
    pub items: Vec<CreatePurchaseOrderItemRequest>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Request to receive an individual purchase order line item
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivePurchaseOrderItemRequest {
    pub product_id: Uuid,
    pub quantity_received: i64,
    #[serde(default)]
    pub unit_cost: Option<Rupiah>,
    #[serde(default)]
    pub batch_number: Option<String>,
}

/// Request to receive goods against an ordered purchase order
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivePurchaseOrderRequest {
    pub items: Vec<ReceivePurchaseOrderItemRequest>,
}

/// Request to cancel a purchase order
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelPurchaseOrderRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Purchase order with nested items for API responses
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderWithItems {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub po_number: String,
    pub supplier_name: String,
    pub destination_warehouse_id: Uuid,
    pub status: PurchaseOrderStatus,
    pub total_amount: Rupiah,
    pub notes: Option<String>,
    pub items: Vec<PurchaseOrderItem>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Wrapper response for purchase orders list
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrdersListResponse {
    pub purchase_orders: Vec<PurchaseOrderWithItems>,
    pub count: usize,
}

/// Result of ordering a purchase order (transition DRAFT -> ORDERED)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderPurchaseOrderResult {
    pub id: Uuid,
    pub status: PurchaseOrderStatus,
    pub updated_at: DateTime<Utc>,
}

/// Result of receiving goods against a purchase order
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivePurchaseOrderResult {
    pub id: Uuid,
    pub po_number: String,
    pub status: PurchaseOrderStatus,
    pub total_receipt_value: Rupiah,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_entry_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

/// Result of cancelling a purchase order
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelPurchaseOrderResult {
    pub id: Uuid,
    pub status: PurchaseOrderStatus,
    pub updated_at: DateTime<Utc>,
}



/// Moving Weighted Average Cost (WAC) calculator (R4)
/// Formula: new_wac = round_half_up_i128((prev_qty * prev_wac + in_qty * in_cost), total_qty)
pub fn calculate_weighted_average_cost(
    prev_qty: i64,
    prev_avg_cost: Rupiah,
    in_qty: i64,
    in_unit_cost: Rupiah,
) -> Result<Rupiah, AppError> {
    if prev_qty < 0 || in_qty <= 0 {
        return Err(AppError::BadRequest(
            "Quantities must be non-negative and incoming quantity must be positive".to_string(),
            "INVALID_QUANTITY",
        ));
    }
    let total_qty = (prev_qty as i128) + (in_qty as i128);
    if total_qty == 0 {
        return Ok(Rupiah::ZERO);
    }
    let total_cost = (prev_qty as i128) * (prev_avg_cost.as_i64() as i128)
        + (in_qty as i128) * (in_unit_cost.as_i64() as i128);
    let new_avg = round_half_up_i128(total_cost, total_qty);
    Ok(Rupiah::new(new_avg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wac_exact_integer_math() {
        // Example: 10 units @ Rp 10.000 + 10 units @ Rp 15.000 = 20 units @ Rp 12.500
        let wac = calculate_weighted_average_cost(
            10,
            Rupiah::new(10_000),
            10,
            Rupiah::new(15_000),
        )
        .unwrap();
        assert_eq!(wac, Rupiah::new(12_500));
    }

    #[test]
    fn test_wac_half_up_rounding() {
        // 4 units @ Rp 12.000 = 48.000 + 3 units @ Rp 10.000 = 30.000 => 78.000 / 7 = 11.142,857...
        // 78000 + 3 = 78003 / 7 = 11143
        let wac = calculate_weighted_average_cost(
            4,
            Rupiah::new(12_000),
            3,
            Rupiah::new(10_000),
        )
        .unwrap();
        assert_eq!(wac, Rupiah::new(11_143));
    }
}
