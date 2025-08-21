use crate::integration_tests::core::*;
use crate::{ThemeMode, WhitenoiseError};
use async_trait::async_trait;

pub struct UpdateThemeModeTestCase {
    theme_mode: ThemeMode,
}

impl UpdateThemeModeTestCase {
    pub fn to_dark() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
        }
    }

    pub fn to_light() -> Self {
        Self {
            theme_mode: ThemeMode::Light,
        }
    }

    pub fn to_system() -> Self {
        Self {
            theme_mode: ThemeMode::System,
        }
    }
}

#[async_trait]
impl TestCase for UpdateThemeModeTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!("Updating theme mode to: {:?}", self.theme_mode);
        context
            .whitenoise
            .update_theme_mode(self.theme_mode.clone())
            .await?;

        // Verify the update worked
        let settings = context.whitenoise.app_settings().await?;
        assert_eq!(
            settings.theme_mode, self.theme_mode,
            "Theme mode was not updated correctly"
        );

        tracing::info!("✓ Theme mode updated and verified: {:?}", self.theme_mode);
        Ok(())
    }
}
