use lightning_invoice::Bolt11Invoice;
use nwc::prelude::*;
use serde::Serialize;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error, Serialize)]
pub enum PaymentError {
    #[error("Invalid invoice format: {0}")]
    InvalidInvoice(String),
    #[error("Invoice has expired")]
    ExpiredInvoice,
    #[error("Invalid NWC URI: {0}")]
    InvalidNwcUri(String),
    #[error("Payment failed: {0}")]
    PaymentFailure(String),
}

/// Pays a bolt11 invoice using the provided NWC URI
///
/// # Arguments
/// * `bolt11` - The bolt11 invoice to pay
/// * `nwc_uri` - The Nostr Wallet Connect URI to use for payment
///
/// # Returns
/// * `Ok(String)` - The payment preimage if successful
/// * `Err(PaymentError)` - The error if payment fails
pub async fn pay_bolt11_invoice(bolt11: &str, nwc_uri: &str) -> Result<String, PaymentError> {
    let invoice =
        Bolt11Invoice::from_str(bolt11).map_err(|e| PaymentError::InvalidInvoice(e.to_string()))?;

    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if current_timestamp > invoice.expires_at().expect("invoice expiry time").as_secs() {
        return Err(PaymentError::ExpiredInvoice);
    }

    let uri = NostrWalletConnectURI::parse(nwc_uri)
        .map_err(|e| PaymentError::InvalidNwcUri(e.to_string()))?;
    let nwc = NWC::new(uri);

    let pay_request = PayInvoiceRequest::new(bolt11.to_string());
    let payment_response = nwc
        .pay_invoice(pay_request)
        .await
        .map_err(|e| PaymentError::PaymentFailure(e.to_string()))?;

    Ok(payment_response.preimage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pay_bolt11_invoice_invalid_bolt11() {
        let result = pay_bolt11_invoice("invalid_bolt11", "nostr+walletconnect://test").await;
        assert!(matches!(result, Err(PaymentError::InvalidInvoice(_))));
    }
}
