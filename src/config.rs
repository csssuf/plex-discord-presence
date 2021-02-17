use std::fs;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const DEFAULT_DISCORD_INTERVAL_MS: u64 = 5000;
const DEFAULT_PLEX_INTERVAL_MS: u64 = 5000;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) plex: PlexConfig,
    pub(crate) discord: DiscordConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PlexConfig {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) server_name: String,
    pub(crate) polling_interval_ms: u64,
}

impl Default for PlexConfig {
    fn default() -> PlexConfig {
        PlexConfig {
            username: String::new(),
            password: String::new(),
            server_name: String::new(),
            polling_interval_ms: DEFAULT_PLEX_INTERVAL_MS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DiscordConfig {
    pub(crate) update_interval_ms: u64,
}

impl Default for DiscordConfig {
    fn default() -> DiscordConfig {
        DiscordConfig {
            update_interval_ms: DEFAULT_DISCORD_INTERVAL_MS,
        }
    }
}

pub(crate) fn load_config() -> Result<Option<Config>, Box<dyn std::error::Error>> {
    let dirs = ProjectDirs::from("", "csssuf", "plex-discord-presence")
        .ok_or("Unable to determine project dirs.")?;

    let config_dir = dirs.config_dir();
    if !config_dir.exists() {
        fs::create_dir(config_dir)?;
    }

    let config_path = config_dir.join("config.toml");

    if !config_path.exists() {
        let default_config = Config::default();
        fs::write(&config_path, toml::to_string_pretty(&default_config)?)?;
        println!("Wrote empty config to {:?}", config_path);

        Ok(None)
    } else {
        let config_contents = fs::read_to_string(&config_path)?;
        Ok(Some(toml::from_str(&config_contents)?))
    }
}
