use crate::integration_tests::core::*;
use crate::{ThemeMode, WhitenoiseError};
use async_trait::async_trait;

pub struct FetchAppSettingsTestCase {
    expected_theme: Option<ThemeMode>,
}

impl FetchAppSettingsTestCase {
    pub fn basic() -> Self {
        Self {
            expected_theme: None,
        }
    }

    pub fn expect_theme(mut self, theme: ThemeMode) -> Self {
        self.expected_theme = Some(theme);
        self
    }
}

#[async_trait]
impl TestCase for FetchAppSettingsTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!("Fetching app settings...");
        let settings = context.whitenoise.app_settings().await?;

        if let Some(expected_theme) = &self.expected_theme {
            assert_eq!(&settings.theme_mode, expected_theme, "Theme mode mismatch");
            tracing::info!("✓ Theme mode verified: {:?}", settings.theme_mode);
        }

        tracing::info!("✓ App settings fetched successfully");
        Ok(())
    }
}
