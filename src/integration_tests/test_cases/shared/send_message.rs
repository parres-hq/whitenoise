use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;
use nostr_sdk::prelude::*;

pub struct SendMessageTestCase {
    message_content: String,
    message_kind: u16,
    sender_account: String,
    target_group: String,
    message_id_key: String,
    tags: Option<Vec<Tag>>,
}

impl SendMessageTestCase {
    pub fn basic() -> Self {
        Self {
            message_content: "Hello from integration test!".to_string(),
            message_kind: 9,
            sender_account: "creator".to_string(),
            target_group: "test_group".to_string(),
            message_id_key: "basic_message".to_string(),
            tags: None,
        }
    }

    pub fn into_reaction(mut self, reaction: &str, target_message_id: &str) -> Self {
        self.message_content = reaction.to_string();
        self.message_kind = 7;
        self.message_id_key = "reaction_message".to_string();
        self.tags = Some(vec![Tag::parse(vec!["e", target_message_id]).unwrap()]);
        self
    }

    pub fn into_reply(mut self, content: &str, target_message_id: &str) -> Self {
        self.message_content = content.to_string();
        self.message_kind = 9;
        self.message_id_key = "reply_message".to_string();
        self.tags = Some(vec![Tag::parse(vec!["e", target_message_id]).unwrap()]);
        self
    }


    pub fn with_content(mut self, content: &str) -> Self {
        self.message_content = content.to_string();
        self
    }

    pub fn with_message_id_key(mut self, key: &str) -> Self {
        self.message_id_key = key.to_string();
        self
    }

    pub fn with_sender(mut self, sender: &str) -> Self {
        self.sender_account = sender.to_string();
        self
    }

    pub fn with_group(mut self, group: &str) -> Self {
        self.target_group = group.to_string();
        self
    }
}

#[async_trait]
impl TestCase for SendMessageTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!("Sending message: {}", self.message_content);

        let sender = context.get_account(&self.sender_account)?;
        let group = context.get_group(&self.target_group)?;

        let result = context
            .whitenoise
            .send_message_to_group(
                sender,
                &group.mls_group_id,
                self.message_content.clone(),
                self.message_kind,
                self.tags.clone(),
            )
            .await?;

        assert_eq!(result.message.content, self.message_content);
        context.add_message_id(&self.message_id_key, result.message.id.to_string());

        tracing::info!("✓ Message sent successfully");
        Ok(())
    }
}
