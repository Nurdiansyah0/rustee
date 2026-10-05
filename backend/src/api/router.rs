use crate::api::handlers::ingestion::ingestion_router;
use crate::api::AppState;
use axum::Router;

#[path = "warehouses.rs"]
pub mod warehouses;

#[path = "products.rs"]
pub mod products;

#[path = "inventory.rs"]
pub mod inventory;

#[path = "purchase_orders.rs"]
pub mod purchase_orders;

pub use inventory::inventory_router;
pub use products::products_router;
pub use purchase_orders::purchase_orders_router;
pub use warehouses::warehouses_router;

/// Registers ingestion pipeline routes under `/ingestion` and inventory routes under
/// `/warehouses`, `/products`, `/inventory`, and `/purchase-orders`.
pub fn register_ingestion_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .nest("/ingestion", ingestion_router())
        .nest("/warehouses", warehouses_router())
        .nest("/products", products_router())
        .nest("/inventory", inventory_router())
        .nest("/purchase-orders", purchase_orders_router())
}

/// Registers inventory routes under `/warehouses`, `/products`, `/inventory`, and `/purchase-orders`.
pub fn register_inventory_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .nest("/warehouses", warehouses_router())
        .nest("/products", products_router())
        .nest("/inventory", inventory_router())
        .nest("/purchase-orders", purchase_orders_router())
}

