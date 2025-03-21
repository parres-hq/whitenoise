use crate::accounts::Account;
use crate::commands::groups::send_mls_message;
use crate::groups::Group;
use crate::payments::{self, PaymentError};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CommandError {
    #[error("Account error: No active account found")]
    NoActiveAccount,
    #[error("Account error: No NWC URI configured")]
    NoNWCUri,
    #[error("Message error: Can not build message")]
    MessageError,
    #[error("Payment error: {0}")]
    PaymentError(String),
}

impl From<PaymentError> for CommandError {
    fn from(err: PaymentError) -> Self {
        Self::PaymentError(err.to_string())
    }
}

#[tauri::command]
pub async fn pay_invoice(
    group: Group,
    tags: Option<Vec<Tag>>,
    bolt11: String,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<UnsignedEvent, CommandError> {
    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|_| CommandError::NoActiveAccount)?;

    let nwc_uri = active_account
        .get_nostr_wallet_connect_uri(wn.clone())
        .map_err(|_| CommandError::NoNWCUri)?
        .ok_or(CommandError::NoNWCUri)?;

    let payment_service = DefaultPaymentService;
    let message_params = pay_invoice_and_get_msg_params(&payment_service, tags, &bolt11, &nwc_uri)
        .await
        .map_err(|_| CommandError::MessageError)?;

    let unsigned_message = send_mls_message(
        group,
        message_params.message,
        message_params.kind,
        message_params.tags,
        None,
        wn,
        app_handle,
    )
    .await
    .map_err(|_| CommandError::MessageError)?;

    Ok(unsigned_message)
}

struct MlsMessageParams {
    message: String,
    kind: u16,
    tags: Option<Vec<Tag>>,
}

#[async_trait::async_trait]
trait PaymentService: Send + Sync {
    /// Pay a BOLT11 invoice and return the preimage
    async fn pay_bolt11_invoice(
        &self,
        bolt11: &str,
        nwc_uri: &str,
    ) -> Result<String, PaymentError> {
        payments::pay_bolt11_invoice(bolt11, nwc_uri).await
    }
}

struct DefaultPaymentService;
impl PaymentService for DefaultPaymentService {}

async fn pay_invoice_and_get_msg_params(
    payment_service: &impl PaymentService,
    tags: Option<Vec<Tag>>,
    bolt11: &str,
    nwc_uri: &str,
) -> Result<MlsMessageParams, CommandError> {
    let preimage = payment_service
        .pay_bolt11_invoice(bolt11, nwc_uri)
        .await
        .map_err(CommandError::from)?;
    Ok(MlsMessageParams {
        message: "".to_string(),
        kind: 9,
        tags: Some(create_payment_tags(tags, &preimage)),
    })
}

fn create_payment_tags(tags: Option<Vec<Tag>>, preimage: &str) -> Vec<Tag> {
    let mut final_tags = tags.unwrap_or_default();
    final_tags.push(Tag::custom(
        TagKind::Custom("preimage".into()),
        vec![preimage.to_string()],
    ));
    final_tags
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPaymentService {
        expected_bolt11: String,
        expected_nwc_uri: String,
        result: Option<String>,
        error_message: Option<String>,
    }

    impl MockPaymentService {
        fn new(bolt11: &str, nwc_uri: &str, result: Result<String, String>) -> Self {
            match result {
                Ok(preimage) => Self {
                    expected_bolt11: bolt11.to_string(),
                    expected_nwc_uri: nwc_uri.to_string(),
                    result: Some(preimage),
                    error_message: None,
                },
                Err(error) => Self {
                    expected_bolt11: bolt11.to_string(),
                    expected_nwc_uri: nwc_uri.to_string(),
                    result: None,
                    error_message: Some(error),
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl PaymentService for MockPaymentService {
        async fn pay_bolt11_invoice(
            &self,
            bolt11: &str,
            nwc_uri: &str,
        ) -> Result<String, PaymentError> {
            assert_eq!(
                bolt11, self.expected_bolt11,
                "bolt11 parameter doesn't match expected value"
            );
            assert_eq!(
                nwc_uri, self.expected_nwc_uri,
                "nwc_uri parameter doesn't match expected value"
            );

            match &self.result {
                Some(preimage) => Ok(preimage.clone()),
                None => {
                    let error_msg = self
                        .error_message
                        .as_ref()
                        .unwrap_or(&"Unknown error".to_string())
                        .clone();
                    Err(PaymentError::PaymentFailure(error_msg))
                }
            }
        }
    }

    #[tokio::test]
    async fn test_pay_invoice_and_get_msg_params_success() {
        let tags = Some(vec![Tag::custom(
            TagKind::Custom("test".into()),
            vec!["value".to_string()],
        )]);
        let bolt11 = "lnbc1invoice";
        let nwc_uri = "nostr+walletconnect://test";
        let expected_preimage = "0123456789abcdef".to_string();

        let mock_service = MockPaymentService::new(bolt11, nwc_uri, Ok(expected_preimage.clone()));
        let result =
            pay_invoice_and_get_msg_params(&mock_service, tags.clone(), bolt11, nwc_uri).await;

        assert!(result.is_ok(), "Expected successful result");
        let params = result.unwrap();
        assert_eq!(params.message, "", "Message should be empty");
        assert_eq!(params.kind, 9, "Kind should be 9");

        let final_tags = params.tags.unwrap();

        let expected_tags = vec![
            tags.clone().unwrap()[0].clone(),
            Tag::custom(
                TagKind::Custom("preimage".into()),
                vec![expected_preimage.clone()],
            ),
        ];

        assert_eq!(
            final_tags.len(),
            expected_tags.len(),
            "Should have 2 tags (original + preimage)"
        );

        for i in 0..expected_tags.len() {
            assert_eq!(
                final_tags[i], expected_tags[i],
                "Tag at position {} doesn't match expected tag",
                i
            );
        }
    }

    #[tokio::test]
    async fn test_pay_invoice_and_get_msg_params_no_tags() {
        let bolt11 = "lnbc1invoice";
        let nwc_uri = "nostr+walletconnect://test";
        let expected_preimage = "0123456789abcdef".to_string();

        let mock_service = MockPaymentService::new(bolt11, nwc_uri, Ok(expected_preimage.clone()));
        let result = pay_invoice_and_get_msg_params(&mock_service, None, bolt11, nwc_uri).await;

        assert!(result.is_ok(), "Expected successful result");
        let params = result.unwrap();
        assert_eq!(params.message, "", "Message should be empty");
        assert_eq!(params.kind, 9, "Kind should be 9");

        let final_tags = params.tags.unwrap();
        assert_eq!(final_tags.len(), 1, "Should have only the preimage tag");

        let expected_tag = Tag::custom(
            TagKind::Custom("preimage".into()),
            vec![expected_preimage.clone()],
        );
        assert_eq!(
            final_tags[0], expected_tag,
            "Tag should be the preimage tag"
        );
    }

    #[tokio::test]
    async fn test_pay_invoice_and_get_msg_params_payment_error() {
        let bolt11 = "lnbc1invoice";
        let nwc_uri = "nostr+walletconnect://test";
        let error_message = "Payment failed".to_string();

        let mock_service = MockPaymentService::new(bolt11, nwc_uri, Err(error_message.clone()));
        let result = pay_invoice_and_get_msg_params(&mock_service, None, bolt11, nwc_uri).await;

        assert!(result.is_err(), "Expected error result");
        match result {
            Err(CommandError::PaymentError(err)) => {
                assert!(
                    err.contains(&error_message),
                    "Error message should contain '{}'",
                    error_message
                );
            }
            _ => panic!("Expected PaymentError"),
        }
    }
}
