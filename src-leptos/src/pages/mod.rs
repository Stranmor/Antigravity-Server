//! Page components

pub(crate) mod accounts;
pub(crate) mod dashboard;
pub(crate) mod login;
pub(crate) mod monitor;
pub(crate) mod proxy;
pub(crate) mod settings;

pub(crate) use accounts::Accounts;
pub(crate) use dashboard::Dashboard;
pub(crate) use login::Login;
pub(crate) use monitor::Monitor;
pub(crate) use proxy::ApiProxy;
pub(crate) use settings::Settings;
