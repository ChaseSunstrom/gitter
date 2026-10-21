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
}

impl Settings {
    pub fn from_config(cfg: &Config) -> Settings {
        Settings {
            instance_name: "Gitter".into(),
            signup: if cfg.signup == SignupMode::Closed {
                "closed".into()
            } else {
                "open".into()
            },
            max_repos_per_user: cfg.max_repos_per_user,
            max_push_mb: cfg.max_push_bytes / (1024 * 1024),
            announcement: String::new(),
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
        if !matches!(self.signup.as_str(), "open" | "closed") {
            return Err(AppError::bad("Sign-up must be open or closed"));
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
