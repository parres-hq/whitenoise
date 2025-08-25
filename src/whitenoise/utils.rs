use nostr_sdk::{types::RelayUrl, PublicKey, ToBech32};

use crate::whitenoise::{error::WhitenoiseError, Whitenoise};

impl Whitenoise {
    /// Converts a Nostr public key to its bech32-encoded npub representation.
    ///
    /// # Arguments
    ///
    /// * `public_key` - A reference to the `PublicKey` to convert
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
    pub fn hex_pubkey_from_npub(npub: &str) -> Result<String, WhitenoiseError> {
        let public_key = PublicKey::parse(npub).map_err(|_| WhitenoiseError::InvalidPublicKey)?;
        Ok(public_key.to_hex())
    }

    /// Capitalizes the first letter of a word, leaving the rest unchanged
    pub(crate) fn capitalize_first_letter(word: &str) -> String {
        let mut chars = word.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize_first_letter() {
        assert_eq!(Whitenoise::capitalize_first_letter("satoshi"), "Satoshi");
        assert_eq!(Whitenoise::capitalize_first_letter("5atoshi"), "5atoshi");
        assert_eq!(Whitenoise::capitalize_first_letter(""), "");
        assert_eq!(Whitenoise::capitalize_first_letter("ßtraße"), "SStraße");
    }

    #[test]
    fn test_hex_pubkey_from_npub() {
        let npub = "npub1jgm0ntzjr03wuzj5788llhed7l6fst05um4ej2r86ueaa08etv6sgd669p";
        let result = Whitenoise::hex_pubkey_from_npub(npub);

        assert!(result.is_ok());
        let hex = result.unwrap();
        assert_eq!(
            hex,
            "9236f9ac521be2ee0a54f1cfffdf2df7f4982df4e6eb992867d733debcf95b35"
        );

        let invalid_npub = "definitely not a valid npub";
        let invalid_result = Whitenoise::hex_pubkey_from_npub(invalid_npub);
        assert!(invalid_result.is_err());

        let empty_npub = "";
        let empty_result = Whitenoise::hex_pubkey_from_npub(empty_npub);
        assert!(empty_result.is_err());
    }

    #[test]
    fn test_npub_from_hex_pubkey() {
        let hex_pubkey = "9236f9ac521be2ee0a54f1cfffdf2df7f4982df4e6eb992867d733debcf95b35";
        let result = Whitenoise::npub_from_hex_pubkey(hex_pubkey);

        assert!(result.is_ok());
        let npub = result.unwrap();
        assert_eq!(
            npub,
            "npub1jgm0ntzjr03wuzj5788llhed7l6fst05um4ej2r86ueaa08etv6sgd669p"
        );

        let invalid_hex = "invalid_hex_string";
        let invalid_result = Whitenoise::npub_from_hex_pubkey(invalid_hex);
        assert!(invalid_result.is_err());

        let empty_hex = "";
        let empty_result = Whitenoise::npub_from_hex_pubkey(empty_hex);
        assert!(empty_result.is_err());
    }

    #[test]
    fn test_npub_from_public_key() {
        let hex_pubkey = "9236f9ac521be2ee0a54f1cfffdf2df7f4982df4e6eb992867d733debcf95b35";
        let public_key = PublicKey::parse(hex_pubkey).unwrap();
        let result = Whitenoise::npub_from_public_key(&public_key);

        assert!(result.is_ok());
        let npub = result.unwrap();
        assert_eq!(
            npub,
            "npub1jgm0ntzjr03wuzj5788llhed7l6fst05um4ej2r86ueaa08etv6sgd669p"
        );
    }
}
