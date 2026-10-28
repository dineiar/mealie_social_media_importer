pub mod auth;
pub mod config;
pub mod error;
pub mod mealie;
mod routes;
pub mod store;

use std::sync::Arc;

use auth::{Authenticator, USER_TTL};
use config::Config;
use mealie::MealieApi;

pub use routes::router;

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    mealie: Arc<Authenticator>,
}

impl AppState {
    pub fn new(config: Config, mealie: Arc<dyn MealieApi>) -> Self {
        Self {
            config: Arc::new(config),
            mealie: Arc::new(Authenticator::new(mealie, USER_TTL)),
        }
    }
}
