pub mod auth;
pub mod config;
pub mod error;
pub mod mealie;
mod routes;

use std::sync::Arc;

use config::Config;

pub use routes::router;

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}
