//! Account management module.

mod async_wrappers;
mod crud;
mod current;
mod fetch;
mod index;
mod paths;
mod quota;
mod storage;
mod switch;
mod verification;

pub use async_wrappers::{load_account_async, save_account_async, update_account_quota_async};
pub use crud::{add_account, delete_account, delete_accounts, reorder_accounts, upsert_account};
pub use current::{get_current_account, get_current_account_id, set_current_account_id};
pub use fetch::{fetch_quota_with_retry, upsert_account_async, QuotaFetchResult};
pub use index::{load_account_index, save_account_index};
pub use paths::{get_accounts_dir, get_data_dir};
pub use quota::update_account_quota;
pub use storage::{list_accounts, load_account, save_account};
pub use switch::switch_account;
pub use verification::mark_needs_verification_by_email;
