use nostr_sdk::PublicKey;

use crate::whitenoise::{Whitenoise, accounts::Account, error::Result, users::User};

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
    pub async fn follow_user(&self, account: &Account, pubkey: &PublicKey) -> Result<()> {
        let (user, newly_created) = User::find_or_create_by_pubkey(pubkey, &self.database).await?;

        if newly_created {
            self.background_fetch_user_data(&user).await?;
        }

        account.follow_user(&user, &self.database).await?;
        self.background_publish_account_follow_list(account).await?;
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
    pub async fn unfollow_user(&self, account: &Account, pubkey: &PublicKey) -> Result<()> {
        let (user, _) = User::find_or_create_by_pubkey(pubkey, &self.database).await?;
        account.unfollow_user(&user, &self.database).await?;
        self.background_publish_account_follow_list(account).await?;
        Ok(())
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
    pub async fn is_following_user(&self, account: &Account, pubkey: &PublicKey) -> Result<bool> {
        let user = self.find_user_by_pubkey(pubkey).await;
        if user.is_err() {
            return Ok(false);
        }
        account
            .is_following_user(&user.unwrap(), &self.database)
            .await
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
    pub async fn follows(&self, account: &Account) -> Result<Vec<User>> {
        account.follows(&self.database).await
    }
}
