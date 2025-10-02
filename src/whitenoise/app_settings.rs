use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Whitenoise, whitenoise::Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::System
    }
}

impl fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeMode::Light => write!(f, "light"),
            ThemeMode::Dark => write!(f, "dark"),
            ThemeMode::System => write!(f, "system"),
        }
    }
}

impl FromStr for ThemeMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "light" => Ok(ThemeMode::Light),
            "dark" => Ok(ThemeMode::Dark),
            "system" => Ok(ThemeMode::System),
            _ => Err(format!("Invalid theme mode: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub id: i64,
    pub theme_mode: ThemeMode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            id: 1,
            theme_mode: ThemeMode::System,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl AppSettings {
    pub fn new(theme_mode: ThemeMode) -> Self {
        Self {
            id: 1, // Always use id=1 since we only allow one row
            theme_mode,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl Whitenoise {
    /// Loads the current application settings from the database.
    ///
    /// This method retrieves the global application settings, which includes
    /// theme preferences and other UI configuration. If no settings exist
    /// in the database, default settings will be created and saved.
    pub async fn app_settings(&self) -> Result<AppSettings> {
        AppSettings::find_or_create_default(&self.database).await
    }

    /// Updates only the theme mode in the application settings.
    ///
    /// This is a convenience method that loads the current settings,
    /// updates only the theme mode, and saves the settings back to the
    /// database. This is more efficient than manually loading, modifying,
    /// and saving when you only need to change the theme.
    ///
    /// # Arguments
    ///
    /// * `theme_mode` - The new [`ThemeMode`] to set (Light, Dark, or System)
    pub async fn update_theme_mode(&self, theme_mode: ThemeMode) -> Result<()> {
        AppSettings::update_theme_mode(theme_mode, &self.database).await
    }
}
