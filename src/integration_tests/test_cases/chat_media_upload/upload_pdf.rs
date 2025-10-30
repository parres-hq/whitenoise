use crate::WhitenoiseError;
use crate::integration_tests::core::*;
use async_trait::async_trait;
use nostr_sdk::Url;

/// Test case for uploading PDF documents
pub struct UploadPdfTestCase {
    account_name: String,
    group_name: String,
}

impl UploadPdfTestCase {
    pub fn new(account_name: &str, group_name: &str) -> Self {
        Self {
            account_name: account_name.to_string(),
            group_name: group_name.to_string(),
        }
    }

    /// Create a temporary PDF file with valid magic bytes
    fn create_test_pdf(&self) -> Result<tempfile::NamedTempFile, WhitenoiseError> {
        use std::io::Write;

        let mut temp_file = tempfile::NamedTempFile::new().map_err(|e| {
            WhitenoiseError::Other(anyhow::anyhow!("Failed to create temp file: {}", e))
        })?;

        // Minimal valid PDF header
        let pdf_header: &[u8] = b"%PDF-1.4\n";

        temp_file.write_all(pdf_header).map_err(|e| {
            WhitenoiseError::Other(anyhow::anyhow!("Failed to write PDF data: {}", e))
        })?;

        temp_file.flush().map_err(|e| {
            WhitenoiseError::Other(anyhow::anyhow!("Failed to flush temp file: {}", e))
        })?;

        Ok(temp_file)
    }
}

#[async_trait]
impl TestCase for UploadPdfTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Uploading PDF document for group {} using account: {}",
            self.group_name,
            self.account_name
        );

        let account = context.get_account(&self.account_name)?;
        let group = context.get_group(&self.group_name)?;

        let temp_file = self.create_test_pdf()?;
        let temp_path = temp_file
            .path()
            .to_str()
            .ok_or_else(|| WhitenoiseError::Other(anyhow::anyhow!("Invalid temp path")))?;

        let blossom_url = if cfg!(debug_assertions) {
            Some(Url::parse("http://localhost:3000").unwrap())
        } else {
            None
        };

        let media_file = context
            .whitenoise
            .upload_chat_media(account, &group.mls_group_id, temp_path, blossom_url, None)
            .await?;

        drop(temp_file);

        tracing::info!(
            "✓ PDF uploaded successfully: hash={}",
            hex::encode(&media_file.file_hash)
        );

        // Validate PDF upload
        assert!(!media_file.file_hash.is_empty());
        assert!(media_file.blossom_url.is_some());
        assert_eq!(media_file.mime_type, "application/pdf");
        assert_eq!(media_file.media_type, "chat_media");
        assert!(media_file.file_path.exists());

        tracing::info!("✓ PDF upload validations passed");

        Ok(())
    }
}
