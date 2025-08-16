use nostr::key::PublicKey;

use crate::whitenoise::{accounts::Account, error::Result, users::User, Whitenoise};

impl Whitenoise {
    /// Creates a follow relationship between an account and a user.
    ///
    /// This method establishes a follow relationship by creating an entry in the `account_follows`
    /// table, linking the account to the user. The user is looked up by their public key, and the
    /// relationship is timestamped with the current time for both creation and update timestamps.
    ///
    /// # Arguments
    ///
    /// * `account` - The account that will follow the user (must exist in database with valid ID)
    /// * `pubkey` - The public key of the user to be followed
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the follow relationship was successfully created.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::whitenoise::WhitenoiseError`] if:
    /// * The account lacks a valid database ID
    /// * The user with the specified public key is not found
    /// * Database operation fails (e.g., connection issues, constraint violations)
    /// * The follow relationship already exists (depending on database constraints)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let account = Account::find_by_pubkey(&account_pubkey, &whitenoise).await?;
    /// let user_pubkey = PublicKey::parse("npub1...").unwrap();
    ///
    /// whitenoise.follow_user(&account, &user_pubkey).await?;
    /// ```
    pub async fn follow_user(&self, account: &Account, pubkey: &PublicKey) -> Result<()> {
        let (user, newly_created) = User::find_or_create_by_pubkey(pubkey, &self.database).await?;
        if newly_created {
            // try and populate the user's data
        }

        account.follow_user(&user, &self.database).await?;

        if newly_created {
            // publish account's follow list to nostr
        }
        Ok(())
    }

    /// Removes a follow relationship between an account and a user.
    ///
    /// This method removes an existing follow relationship by deleting the corresponding
    /// entry from the `account_follows` table. The user is looked up by their public key.
    /// If no relationship exists, the operation succeeds without error.
    ///
    /// # Arguments
    ///
    /// * `account` - The account that will unfollow the user (must exist in database with valid ID)
    /// * `pubkey` - The public key of the user to be unfollowed
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the unfollow operation completed successfully, regardless of
    /// whether a follow relationship previously existed.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::whitenoise::WhitenoiseError`] if:
    /// * The account lacks a valid database ID
    /// * The user with the specified public key is not found
    /// * Database operation fails (e.g., connection issues)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let account = Account::find_by_pubkey(&account_pubkey, &whitenoise).await?;
    /// let user_pubkey = PublicKey::parse("npub1...").unwrap();
    ///
    /// whitenoise.unfollow_user(&account, &user_pubkey).await?;
    /// ```
    pub async fn unfollow_user(&self, account: &Account, pubkey: &PublicKey) -> Result<()> {
        let user = self.find_user_by_pubkey(pubkey).await?;
        account.unfollow_user(&user, &self.database).await
    }

    /// Checks if an account is following a specific user.
    ///
    /// This method queries the `account_follows` table to determine whether a follow
    /// relationship exists between the specified account and user. The user is looked
    /// up by their public key.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to check (must exist in database with valid ID)
    /// * `pubkey` - The public key of the user to check if followed
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the account is following the user, `Ok(false)` otherwise.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::whitenoise::WhitenoiseError`] if:
    /// * The account lacks a valid database ID
    /// * The user with the specified public key is not found
    /// * Database query fails (e.g., connection issues)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let account = Account::find_by_pubkey(&account_pubkey, &whitenoise).await?;
    /// let user_pubkey = PublicKey::parse("npub1...").unwrap();
    ///
    /// let is_following = whitenoise.is_following_user(&account, &user_pubkey).await?;
    /// if is_following {
    ///     println!("Account is following this user");
    /// }
    /// ```
    pub async fn is_following_user(&self, account: &Account, pubkey: &PublicKey) -> Result<bool> {
        let user = self.find_user_by_pubkey(pubkey).await?;
        account.is_following_user(&user, &self.database).await
    }

    /// Retrieves all users that an account follows.
    ///
    /// This method queries the `account_follows` table to get a complete list of users
    /// that the specified account is following. The returned users include their full
    /// metadata and profile information.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose follows to retrieve (must exist in database with valid ID)
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<User>)` containing all users that the account follows.
    /// Returns an empty vector if the account follows no users.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::whitenoise::WhitenoiseError`] if:
    /// * The account lacks a valid database ID
    /// * Database query fails (e.g., connection issues)
    /// * Account is not found in the database
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let account = Account::find_by_pubkey(&account_pubkey, &whitenoise).await?;
    ///
    /// let followed_users = whitenoise.follows(&account).await?;
    /// for user in followed_users {
    ///     println!("Following user: {}", user.pubkey.to_hex());
    /// }
    /// ```
    pub async fn follows(&self, account: &Account) -> Result<Vec<User>> {
        account.follows(&self.database).await
    }

    pub async fn follow_users(&self, account: &Account, pubkeys: &[PublicKey]) -> Result<()> {
        let mut users = Vec::new();
        for pubkey in pubkeys {
            let user = self.find_user_by_pubkey(pubkey).await?;
            users.push(user);
        }
        account.follow_users(&users, &self.database).await
    }
}
