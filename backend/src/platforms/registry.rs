use std::sync::Arc;

use url::Url;

use super::{Instagram, Platform, PostLink, Rejected};
use crate::config::Config;

pub struct Registry {
    platforms: Vec<Arc<dyn Platform>>,
}

pub struct Resolved {
    pub platform: Arc<dyn Platform>,
    pub link: PostLink,
}

impl Registry {
    pub fn new(platforms: Vec<Arc<dyn Platform>>) -> Self {
        Self { platforms }
    }

    pub fn from_config(config: &Config) -> Self {
        let cookies_dir = config.data_dir.join("cookies");
        let instagram = Instagram::new(&cookies_dir, config.instagram_cookies_file.clone());
        Self::new(vec![Arc::new(instagram)])
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Platform>> {
        self.platforms.iter()
    }

    pub fn resolve(&self, input: &str) -> Result<Resolved, Rejected> {
        let url = Url::parse(input).map_err(|_| Rejected::UnsupportedHost)?;
        let platform = self
            .platforms
            .iter()
            .find(|platform| platform.matches(&url))
            .ok_or(Rejected::UnsupportedHost)?;
        let link = platform.canonicalize(&url)?;
        Ok(Resolved {
            platform: Arc::clone(platform),
            link,
        })
    }
}
