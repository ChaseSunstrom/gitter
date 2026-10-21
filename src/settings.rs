//! Instance settings an admin can change in the web UI. Environment variables
//! give the starting values; anything saved in the UI wins from then on.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{AppError, R};
use crate::{Config, SignupMode};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Shown in the top bar and page titles.
    pub instance_name: String,
    /// "open" or "closed".
    pub signup: String,
    pub max_repos_per_user: i64,
    pub max_push_mb: u64,
    /// A short notice shown to every visitor (maintenance, rules). Empty hides it.
    pub announcement: String,
    /// Disk space each account's repositories may use, in MB (0 = no limit; admins have none).
    #[serde(default = "default_storage")]
    pub max_storage_mb: u64,
    /// API requests per minute from one address (0 = no limit).
    #[serde(default = "default_api_rate")]
    pub api_rate_per_min: u32,
    /// Changes (posts, edits, pushes) per minute from one account (0 = no limit).
    #[serde(default = "default_write_rate")]
    pub write_rate_per_min: u32,
}

fn default_storage() -> u64 {
    2048
}
fn default_api_rate() -> u32 {
    1200
}
fn default_write_rate() -> u32 {
    120
}

impl Settings {
    pub fn from_config(cfg: &Config) -> Settings {
        Settings {
            instance_name: "Gitter".into(),
            signup: match cfg.signup {
                SignupMode::Open => "open",
                SignupMode::Invite => "invite",
                SignupMode::Closed => "closed",
            }
            .into(),
            max_repos_per_user: cfg.max_repos_per_user,
            max_push_mb: cfg.max_push_bytes / (1024 * 1024),
            announcement: String::new(),
            max_storage_mb: default_storage(),
            api_rate_per_min: default_api_rate(),
            write_rate_per_min: default_write_rate(),
        }
    }

    pub fn signup_open(&self) -> bool {
        self.signup == "open"
    }

    pub fn max_push_bytes(&self) -> u64 {
        self.max_push_mb * 1024 * 1024
    }

    pub fn validate(&self) -> R<()> {
        if self.instance_name.trim().is_empty() || self.instance_name.chars().count() > 40 {
            return Err(AppError::bad("The instance name is 1 to 40 characters"));
        }
        if !matches!(self.signup.as_str(), "open" | "invite" | "closed") {
            return Err(AppError::bad("Sign-up must be open, invite-only or closed"));
        }
        if self.max_storage_mb > 10_000_000 {
            return Err(AppError::bad("Storage per account must be under 10 TB (0 means no limit)"));
        }
        if !(1..=100_000).contains(&self.max_repos_per_user) {
            return Err(AppError::bad(
                "Repositories per user must be between 1 and 100,000",
            ));
        }
        if !(1..=100_000).contains(&self.max_push_mb) {
            return Err(AppError::bad(
                "The push limit must be between 1 MB and 100 GB",
            ));
        }
        if self.announcement.chars().count() > 280 {
            return Err(AppError::bad("Keep the announcement under 280 characters"));
        }
        Ok(())
    }
}

/// The current settings, shared by every request.
pub struct Live(RwLock<Settings>);

impl Live {
    pub fn new(s: Settings) -> Live {
        Live(RwLock::new(s))
    }

    pub fn get(&self) -> Settings {
        self.0.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Overlay whatever was saved in the database onto the defaults.
    pub async fn load(&self, db: &Db) -> R<()> {
        let saved: Option<String> = db
            .call(|c| {
                use rusqlite::OptionalExtension;
                Ok(c.query_row(
                    "SELECT value FROM settings WHERE key = 'instance'",
                    [],
                    |r| r.get(0),
                )
                .optional()?)
            })
            .await?;
        if let Some(json) = saved {
            let mut base = serde_json::to_value(self.get()).map_err(AppError::internal)?;
            if let (Some(b), Ok(serde_json::Value::Object(over))) = (
                base.as_object_mut(),
                serde_json::from_str::<serde_json::Value>(&json),
            ) {
                b.extend(over);
            }
            if let Ok(s) = serde_json::from_value::<Settings>(base) {
                *self.0.write().unwrap_or_else(|p| p.into_inner()) = s;
            }
        }
        Ok(())
    }

    pub async fn save(&self, db: &Db, s: Settings) -> R<()> {
        s.validate()?;
        let json = serde_json::to_string(&s).map_err(AppError::internal)?;
        db.call(move |c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES ('instance', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [json],
            )?;
            Ok(())
        })
        .await?;
        *self.0.write().unwrap_or_else(|p| p.into_inner()) = s;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn saved_settings_win_over_the_environment_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("g.db");
        let env = Settings { signup: "open".into(), max_push_mb: 500, ..Settings::from_config(&test_cfg()) };
        let live = Live::new(env.clone());
        let saved = Settings { signup: "closed".into(), announcement: "Hi".into(), ..env.clone() };
        live.save(&Db::open(&path).unwrap(), saved.clone()).await.unwrap();
        // A fresh process: defaults from the environment, then what was saved.
        let after = Live::new(env);
        after.load(&Db::open(&path).unwrap()).await.unwrap();
        assert_eq!(after.get(), saved);
    }

    fn test_cfg() -> Config {
        Config {
            data_dir: std::env::temp_dir(),
            addr: String::new(),
            url: String::new(),
            secure_cookies: false,
            trust_proxy: false,
            signup: SignupMode::Open,
            max_push_bytes: 1 << 20,
            max_repos_per_user: 10,
        }
    }
}
