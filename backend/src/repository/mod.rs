pub mod account_repo;
pub mod accounting_repo;
pub mod audit_repo;
pub mod category_repo;
pub mod db;
pub mod error;
pub mod idempotency_repo;
pub mod ingestion_repo;
pub mod inventory_repo;
pub mod invoice_repo;
pub mod outbox_repo;
pub mod project_repo;
pub mod receivable_repo;
pub mod subscription_repo;
pub mod tenant_repo;
pub mod transaction_repo;
pub mod user_preferences_repo;
pub mod user_repo;

pub use account_repo::{Account, AccountRepository, NewAccount, SqlxAccountRepository};
pub use inventory_repo::{InventoryRepository, SqlxInventoryRepository};
pub use invoice_repo::{InvoiceRepository, SqlxInvoiceRepository};
pub use outbox_repo::{OutboxRepository, SqlxOutboxRepository};
pub use project_repo::{
    ProjectFilter, ProjectMemberWithUser, ProjectRepository, SqlxProjectRepository, TaskFilter,
};
pub use receivable_repo::{ReceivableRepository, SqlxReceivableRepository};
pub use accounting_repo::{
    AccountingRepository, ChartOfAccountsRepository, JournalRepository, SqlxAccountingRepository,
    TrialBalanceAccountDto, TrialBalanceDto,
};
pub use audit_repo::{AuditLog, AuditRepository, NewAuditLog, SqlxAuditRepository};
pub use category_repo::{Category, CategoryRepository, NewCategory, SqlxCategoryRepository};
pub use db::{
    ensure_parent_dir_exists, init_pool, run_migrations, verify_pragmas, DbConfig, PragmaStatus,
};
pub use error::DbError;
pub use idempotency_repo::{
    IdempotencyLockResult, IdempotencyRecord, IdempotencyRepository, SqlxIdempotencyRepository,
};
pub use ingestion_repo::{
    CreateIngestedTxParams, IngestionEventRecord, IngestionRepository, NewIngestionEvent,
    SqlxIngestionRepository,
};
pub use subscription_repo::{
    NewSubscription, NewWebhookEvent, SqlxSubscriptionRepository, Subscription,
    SubscriptionRepository, WebhookEvent,
};
pub use tenant_repo::{
    BusinessProfileRepository, MembershipRepository, MembershipWithUser, NewMembership, NewTenant,
    SqlxBusinessProfileRepository, SqlxMembershipRepository, SqlxTenantRepository,
    TenantRepository, TenantWithRole, UpsertBusinessProfile,
};
pub use transaction_repo::{
    CashFlowSummary, NewTransaction, SqlxTransactionRepository, TransactionFilter,
    TransactionRecord, TransactionRepository,
};
pub use user_preferences_repo::{
    SqlxUserPreferencesRepository, UpdateUserPreferences, UserPreferences,
    UserPreferencesRepository,
};
pub use user_repo::{NewUser, SqlxUserRepository, User, UserRepository};
