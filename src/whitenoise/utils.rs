use crate::whitenoise::error::WhitenoiseError;
use nostr::{types::RelayUrl, PublicKey, ToBech32};

use super::Whitenoise;

impl Whitenoise {
    /// Converts a Nostr public key to its bech32-encoded npub representation.
    ///
    /// # Arguments
    ///
    /// * `public_key` - A reference to the `PublicKey` to convert
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - The bech32-encoded npub string (starts with "npub1")
    /// * `Err(WhitenoiseError::InvalidPublicKey)` - If the conversion to bech32 fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use nostr::PublicKey;
    /// # use whitenoise::{Whitenoise, WhitenoiseError};
    /// # fn main() -> Result<(), WhitenoiseError> {
    /// let hex_pubkey = "32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245";
    /// let public_key = PublicKey::parse(hex_pubkey).map_err(|_| WhitenoiseError::InvalidPublicKey)?;
    /// let npub = Whitenoise::npub_from_public_key(&public_key)?;
    /// println!("npub: {}", npub);
    /// # Ok(())
    /// # }
    /// ```
    pub fn npub_from_public_key(public_key: &PublicKey) -> Result<String, WhitenoiseError> {
        public_key
            .to_bech32()
            .map_err(|_| WhitenoiseError::InvalidPublicKey)
    }

    /// Converts a hex-encoded public key string to its bech32-encoded npub representation.
    ///
    /// This is a convenience method that first parses the hex string into a `PublicKey`
    /// and then converts it to the npub format.
    ///
    /// # Arguments
    ///
    /// * `hex_pubkey` - A hex-encoded public key string (64 characters)
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - The bech32-encoded npub string (starts with "npub1")
    /// * `Err(WhitenoiseError::InvalidPublicKey)` - If the hex string is invalid or conversion fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use whitenoise::{Whitenoise, WhitenoiseError};
    /// # fn main() -> Result<(), WhitenoiseError> {
    /// let hex_pubkey = "32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245";
    /// let npub = Whitenoise::npub_from_hex_pubkey(hex_pubkey)?;
    /// println!("npub: {}", npub);
    /// # Ok(())
    /// # }
    /// ```
    pub fn npub_from_hex_pubkey(hex_pubkey: &str) -> Result<String, WhitenoiseError> {
        let public_key =
            PublicKey::parse(hex_pubkey).map_err(|_| WhitenoiseError::InvalidPublicKey)?;
        Self::npub_from_public_key(&public_key)
    }

    /// Converts a bech32-encoded npub string to its hex representation.
    ///
    /// This method parses an npub string and returns the underlying public key
    /// as a hex-encoded string.
    ///
    /// # Arguments
    ///
    /// * `npub` - A bech32-encoded npub string (starts with "npub1")
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - The hex-encoded public key string (64 characters)
    /// * `Err(WhitenoiseError::InvalidPublicKey)` - If the npub string is invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use whitenoise::{Whitenoise, WhitenoiseError};
    /// # fn main() -> Result<(), WhitenoiseError> {
    /// let npub = "npub1xt0c0fk652s7hk795jny78ru0w642wrp5fj8ul0nawl6njmx6fzs7fyz88";
    /// let hex_pubkey = Whitenoise::hex_pubkey_from_npub(npub)?;
    /// println!("hex pubkey: {}", hex_pubkey);
    /// # Ok(())
    /// # }
    /// ```
    pub fn hex_pubkey_from_npub(npub: &str) -> Result<String, WhitenoiseError> {
        let public_key = PublicKey::parse(npub).map_err(|_| WhitenoiseError::InvalidPublicKey)?;
        Ok(public_key.to_hex())
    }

    pub fn parse_relays_from_sql(
        relays: String,
    ) -> core::result::Result<Vec<RelayUrl>, sqlx::Error> {
        serde_json::from_str(&relays)
            .map(|urls: Vec<String>| {
                urls.iter()
                    .filter_map(|url| RelayUrl::parse(url).ok())
                    .collect::<Vec<_>>()
            })
            .map_err(|e| sqlx::Error::ColumnDecode {
                index: "nip65_relays".to_owned(),
                source: Box::new(e),
            })
    }
}
