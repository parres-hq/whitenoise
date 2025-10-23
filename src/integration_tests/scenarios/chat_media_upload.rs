use crate::integration_tests::{
    core::*,
    test_cases::{chat_media_upload::*, shared::*},
};
use crate::{Whitenoise, WhitenoiseError};
use async_trait::async_trait;

pub struct ChatMediaUploadScenario {
    context: ScenarioContext,
}

impl ChatMediaUploadScenario {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            context: ScenarioContext::new(whitenoise),
        }
    }
}

#[async_trait]
impl Scenario for ChatMediaUploadScenario {
    fn context(&self) -> &ScenarioContext {
        &self.context
    }

    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
        // Create test accounts
        CreateAccountsTestCase::with_names(vec!["media_uploader", "media_member"])
            .execute(&mut self.context)
            .await?;

        // Create a test group for media uploads
        CreateGroupTestCase::basic()
            .with_name("media_upload_test_group")
            .with_members("media_uploader", vec!["media_member"])
            .execute(&mut self.context)
            .await?;

        // Upload image with default options (includes blurhash generation)
        UploadChatMediaTestCase::basic()
            .with_account("media_uploader")
            .with_group("media_upload_test_group")
            .execute(&mut self.context)
            .await?;

        // Send a message that references the uploaded media and verify aggregation links it
        SendMessageWithMediaTestCase::new("media_uploader", "media_upload_test_group")
            .execute(&mut self.context)
            .await?;

        tracing::info!("✓ Chat media upload scenario completed with:");
        tracing::info!("  • Image upload with default processing options");
        tracing::info!("  • Blurhash generation verification");
        tracing::info!("  • Metadata extraction and storage");
        tracing::info!("  • Message with media reference sent");
        tracing::info!("  • Message aggregation verified media linking");

        Ok(())
    }
}
