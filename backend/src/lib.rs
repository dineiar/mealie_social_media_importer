#![forbid(unsafe_code)]

pub mod auth;
pub mod config;
pub mod error;
pub mod mealie;
pub mod platforms;
pub mod process;
mod routes;
pub mod store;
pub mod tools;

use std::sync::Arc;

use auth::{Authenticator, USER_TTL};
use config::Config;
use mealie::MealieApi;
use platforms::Registry;
use store::{ImportStore, SystemClock};

pub use routes::router;

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    mealie: Arc<Authenticator>,
    imports: Arc<ImportStore>,
    platforms: Arc<Registry>,
}

impl AppState {
    pub fn new(config: Config, mealie: Arc<dyn MealieApi>) -> Self {
        let imports = ImportStore::new(
            &config.data_dir,
            config.media_retention,
            Arc::new(SystemClock),
        );
        Self {
            platforms: Arc::new(Registry::from_config(&config)),
            config: Arc::new(config),
            mealie: Arc::new(Authenticator::new(mealie, USER_TTL)),
            imports: Arc::new(imports),
        }
    }

    pub fn with_imports(self, imports: Arc<ImportStore>) -> Self {
        Self { imports, ..self }
    }
}
