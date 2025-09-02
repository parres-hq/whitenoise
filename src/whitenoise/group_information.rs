use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use nostr_mls::prelude::GroupId;
use serde::{Deserialize, Serialize};

use crate::whitenoise::{Whitenoise, WhitenoiseError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GroupType {
    Group,
    DirectMessage,
}

impl Default for GroupType {
    fn default() -> Self {
        Self::Group
    }
}

impl fmt::Display for GroupType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupType::Group => write!(f, "group"),
            GroupType::DirectMessage => write!(f, "direct_message"),
        }
    }
}

impl FromStr for GroupType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "group" => Ok(GroupType::Group),
            "direct_message" => Ok(GroupType::DirectMessage),
            _ => Err(format!("Invalid group type: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupInformation {
    pub id: Option<i64>,
    pub mls_group_id: GroupId,
    pub group_type: GroupType,
    pub image_pointer: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl GroupInformation {
    fn infer_group_type_from_participant_count(participant_count: usize) -> GroupType {
        match participant_count {
            2 => GroupType::DirectMessage,
            _ => GroupType::Group,
        }
    }
    /// Creates a new GroupInformation with the specified or inferred group type
    ///
    /// # Arguments
    /// * `mls_group_id` - The MLS group ID
    /// * `group_type` - Optional explicit group type. If None, will be inferred from participant count
    /// * `participant_count` - Total number of participants including the creator (used for inference if group_type is None)
    /// * `whitenoise` - Reference to the Whitenoise instance for database operations
    pub async fn create_for_group(
        whitenoise: &Whitenoise,
        mls_group_id: &GroupId,
        group_type: Option<GroupType>,
        image_pointer: Option<String>,
        participant_count: usize,
    ) -> Result<GroupInformation, WhitenoiseError> {
        let group_type = group_type
            .unwrap_or_else(|| Self::infer_group_type_from_participant_count(participant_count));
        let (group_info, _was_created) = Self::find_or_create_by_mls_group_id(
            mls_group_id,
            Some(group_type),
            image_pointer,
            &whitenoise.database,
        )
        .await?;
        Ok(group_info)
    }

    /// Get group information by MLS group ID, creating it with default type if it doesn't exist
    pub async fn get_by_mls_group_id(
        mls_group_id: &GroupId,
        whitenoise: &Whitenoise,
    ) -> Result<GroupInformation, WhitenoiseError> {
        let (group_info, _was_created) = GroupInformation::find_or_create_by_mls_group_id(
            mls_group_id,
            Some(GroupType::default()),
            None,
            &whitenoise.database,
        )
        .await?;
        Ok(group_info)
    }

    /// Get group information for multiple MLS group IDs
    /// Missing groups will be created with default type (Group)
    pub async fn get_by_mls_group_ids(
        mls_group_ids: &[GroupId],
        whitenoise: &Whitenoise,
    ) -> Result<Vec<GroupInformation>, WhitenoiseError> {
        // First try to get existing records
        let existing =
            GroupInformation::find_by_mls_group_ids(mls_group_ids, &whitenoise.database).await?;

        // If we got all the records we need, return them
        if existing.len() == mls_group_ids.len() {
            return Ok(existing);
        }

        // Otherwise, we need to create missing ones
        let mut existing_map: std::collections::HashMap<GroupId, GroupInformation> = existing
            .into_iter()
            .map(|gi| (gi.mls_group_id.clone(), gi))
            .collect();

        let mut results = Vec::new();
        for mls_group_id in mls_group_ids {
            if let Some(existing_info) = existing_map.remove(mls_group_id) {
                results.push(existing_info);
            } else {
                // Create missing record with default type
                let (new_info, _was_created) = GroupInformation::find_or_create_by_mls_group_id(
                    mls_group_id,
                    Some(GroupType::default()),
                    None,
                    &whitenoise.database,
                )
                .await?;
                results.push(new_info);
            }
        }

        Ok(results)
    }
}

impl Whitenoise {
    pub async fn get_group_information_by_mls_group_id(
        &self,
        mls_group_id: &GroupId,
    ) -> Result<GroupInformation, WhitenoiseError> {
        GroupInformation::get_by_mls_group_id(mls_group_id, self).await
    }

    pub async fn get_group_information_by_mls_group_ids(
        &self,
        mls_group_ids: &[GroupId],
    ) -> Result<Vec<GroupInformation>, WhitenoiseError> {
        GroupInformation::get_by_mls_group_ids(mls_group_ids, self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::create_mock_whitenoise;

    #[test]
    fn test_group_type_default() {
        assert_eq!(GroupType::default(), GroupType::Group);
    }

    #[test]
    fn test_group_type_display() {
        assert_eq!(GroupType::Group.to_string(), "group");
        assert_eq!(GroupType::DirectMessage.to_string(), "direct_message");
    }

    #[test]
    fn test_group_type_from_str() {
        assert_eq!(GroupType::from_str("group").unwrap(), GroupType::Group);
        assert_eq!(GroupType::from_str("Group").unwrap(), GroupType::Group);
        assert_eq!(GroupType::from_str("GROUP").unwrap(), GroupType::Group);

        assert_eq!(
            GroupType::from_str("direct_message").unwrap(),
            GroupType::DirectMessage
        );
        assert_eq!(
            GroupType::from_str("Direct_Message").unwrap(),
            GroupType::DirectMessage
        );
        assert_eq!(
            GroupType::from_str("DIRECT_MESSAGE").unwrap(),
            GroupType::DirectMessage
        );

        assert!(GroupType::from_str("invalid").is_err());
        assert!(GroupType::from_str("").is_err());
        assert!(GroupType::from_str("dm").is_err());
    }

    #[test]
    fn test_group_type_from_str_error_message() {
        let result = GroupType::from_str("invalid");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid group type: invalid");
    }

    #[test]
    fn test_group_type_serialization() {
        let group_type = GroupType::Group;
        let serialized = serde_json::to_string(&group_type).unwrap();
        let deserialized: GroupType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(group_type, deserialized);

        let dm_type = GroupType::DirectMessage;
        let serialized = serde_json::to_string(&dm_type).unwrap();
        let deserialized: GroupType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(dm_type, deserialized);
    }

    #[test]
    fn test_infer_group_type_from_participant_count() {
        assert_eq!(
            GroupInformation::infer_group_type_from_participant_count(1),
            GroupType::Group
        );
        assert_eq!(
            GroupInformation::infer_group_type_from_participant_count(2),
            GroupType::DirectMessage
        );
        assert_eq!(
            GroupInformation::infer_group_type_from_participant_count(3),
            GroupType::Group
        );
        assert_eq!(
            GroupInformation::infer_group_type_from_participant_count(10),
            GroupType::Group
        );
        assert_eq!(
            GroupInformation::infer_group_type_from_participant_count(0),
            GroupType::Group
        );
    }

    #[tokio::test]
    async fn test_create_for_group_with_explicit_type() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[1; 32]);

        let result = GroupInformation::create_for_group(
            &whitenoise,
            &group_id,
            Some(GroupType::DirectMessage),
            None,
            5, // Should be ignored when explicit type provided
        )
        .await;

        assert!(result.is_ok());
        let group_info = result.unwrap();
        assert_eq!(group_info.mls_group_id, group_id);
        assert_eq!(group_info.group_type, GroupType::DirectMessage);
        assert!(group_info.id.is_some());
    }

    #[tokio::test]
    async fn test_create_for_group_with_inferred_type_dm() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[2; 32]);

        let result = GroupInformation::create_for_group(
            &whitenoise,
            &group_id,
            None,
            None,
            2, // Should infer DirectMessage
        )
        .await;

        assert!(result.is_ok());
        let group_info = result.unwrap();
        assert_eq!(group_info.mls_group_id, group_id);
        assert_eq!(group_info.group_type, GroupType::DirectMessage);
        assert!(group_info.id.is_some());
    }

    #[tokio::test]
    async fn test_create_for_group_with_inferred_type_group() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[3; 32]);

        let result = GroupInformation::create_for_group(
            &whitenoise,
            &group_id,
            None,
            None,
            5, // Should infer Group
        )
        .await;

        assert!(result.is_ok());
        let group_info = result.unwrap();
        assert_eq!(group_info.mls_group_id, group_id);
        assert_eq!(group_info.group_type, GroupType::Group);
        assert!(group_info.id.is_some());
    }

    #[tokio::test]
    async fn test_create_for_group_idempotent() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[4; 32]);

        // First call - should create
        let result1 = GroupInformation::create_for_group(
            &whitenoise,
            &group_id,
            Some(GroupType::Group),
            None,
            3,
        )
        .await;
        assert!(result1.is_ok());
        let group_info1 = result1.unwrap();

        // Second call - should find existing (not create new)
        let result2 = GroupInformation::create_for_group(
            &whitenoise,
            &group_id,
            Some(GroupType::DirectMessage), // Different type, but should find existing
            None,
            2,
        )
        .await;
        assert!(result2.is_ok());
        let group_info2 = result2.unwrap();

        // Should be same record (same ID) and preserve original type
        assert_eq!(group_info1.id, group_info2.id);
        assert_eq!(group_info1.group_type, group_info2.group_type);
        assert_eq!(group_info2.group_type, GroupType::Group); // Original type preserved
    }

    #[tokio::test]
    async fn test_get_by_mls_group_id_creates_with_default() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[5; 32]);

        let result = GroupInformation::get_by_mls_group_id(&group_id, &whitenoise).await;

        assert!(result.is_ok());
        let group_info = result.unwrap();
        assert_eq!(group_info.mls_group_id, group_id);
        assert_eq!(group_info.group_type, GroupType::Group); // Default type
        assert!(group_info.id.is_some());
    }

    #[tokio::test]
    async fn test_get_by_mls_group_id_finds_existing() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[6; 32]);

        // First create with specific type
        let original = GroupInformation::create_for_group(
            &whitenoise,
            &group_id,
            Some(GroupType::DirectMessage),
            None,
            2,
        )
        .await
        .unwrap();

        // Get should find the existing one
        let result = GroupInformation::get_by_mls_group_id(&group_id, &whitenoise).await;
        assert!(result.is_ok());
        let found = result.unwrap();

        assert_eq!(original.id, found.id);
        assert_eq!(found.group_type, GroupType::DirectMessage); // Original type preserved
    }

    #[tokio::test]
    async fn test_get_by_mls_group_ids_all_existing() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id1 = GroupId::from_slice(&[7; 32]);
        let group_id2 = GroupId::from_slice(&[8; 32]);

        // Create both groups first
        let _info1 = GroupInformation::create_for_group(
            &whitenoise,
            &group_id1,
            Some(GroupType::Group),
            None,
            5,
        )
        .await
        .unwrap();

        let _info2 = GroupInformation::create_for_group(
            &whitenoise,
            &group_id2,
            Some(GroupType::DirectMessage),
            None,
            2,
        )
        .await
        .unwrap();

        // Get both
        let result = GroupInformation::get_by_mls_group_ids(
            &[group_id1.clone(), group_id2.clone()],
            &whitenoise,
        )
        .await;

        assert!(result.is_ok());
        let group_infos = result.unwrap();
        assert_eq!(group_infos.len(), 2);

        // Check that we got both groups with correct types
        let mut found_group = false;
        let mut found_dm = false;
        for info in &group_infos {
            if info.mls_group_id == group_id1 {
                assert_eq!(info.group_type, GroupType::Group);
                found_group = true;
            } else if info.mls_group_id == group_id2 {
                assert_eq!(info.group_type, GroupType::DirectMessage);
                found_dm = true;
            }
        }
        assert!(found_group && found_dm);
    }

    #[tokio::test]
    async fn test_get_by_mls_group_ids_mixed_existing_and_missing() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id1 = GroupId::from_slice(&[9; 32]);
        let group_id2 = GroupId::from_slice(&[10; 32]);
        let group_id3 = GroupId::from_slice(&[11; 32]);

        // Create only the first one
        let _info1 = GroupInformation::create_for_group(
            &whitenoise,
            &group_id1,
            Some(GroupType::DirectMessage),
            None,
            2,
        )
        .await
        .unwrap();

        // Get all three (one exists, two missing)
        let result = GroupInformation::get_by_mls_group_ids(
            &[group_id1.clone(), group_id2.clone(), group_id3.clone()],
            &whitenoise,
        )
        .await;

        assert!(result.is_ok());
        let group_infos = result.unwrap();
        assert_eq!(group_infos.len(), 3);

        // Check results - should preserve order from input
        assert_eq!(group_infos[0].mls_group_id, group_id1);
        assert_eq!(group_infos[0].group_type, GroupType::DirectMessage); // Existing type preserved

        assert_eq!(group_infos[1].mls_group_id, group_id2);
        assert_eq!(group_infos[1].group_type, GroupType::Group); // Default for new

        assert_eq!(group_infos[2].mls_group_id, group_id3);
        assert_eq!(group_infos[2].group_type, GroupType::Group); // Default for new
    }

    #[tokio::test]
    async fn test_get_by_mls_group_ids_all_missing() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id1 = GroupId::from_slice(&[12; 32]);
        let group_id2 = GroupId::from_slice(&[13; 32]);

        // Get both (neither exists)
        let result = GroupInformation::get_by_mls_group_ids(
            &[group_id1.clone(), group_id2.clone()],
            &whitenoise,
        )
        .await;

        assert!(result.is_ok());
        let group_infos = result.unwrap();
        assert_eq!(group_infos.len(), 2);

        // Both should be created with default type
        assert_eq!(group_infos[0].mls_group_id, group_id1);
        assert_eq!(group_infos[0].group_type, GroupType::Group);

        assert_eq!(group_infos[1].mls_group_id, group_id2);
        assert_eq!(group_infos[1].group_type, GroupType::Group);
    }

    #[tokio::test]
    async fn test_get_by_mls_group_ids_empty_input() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let result = GroupInformation::get_by_mls_group_ids(&[], &whitenoise).await;

        assert!(result.is_ok());
        let group_infos = result.unwrap();
        assert!(group_infos.is_empty());
    }

    #[tokio::test]
    async fn test_get_by_mls_group_ids_preserves_order() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id1 = GroupId::from_slice(&[14; 32]);
        let group_id2 = GroupId::from_slice(&[15; 32]);
        let group_id3 = GroupId::from_slice(&[16; 32]);

        // Test order preservation when all are missing
        let result = GroupInformation::get_by_mls_group_ids(
            &[group_id2.clone(), group_id1.clone(), group_id3.clone()], // Intentional different order
            &whitenoise,
        )
        .await;

        assert!(result.is_ok());
        let group_infos = result.unwrap();
        assert_eq!(group_infos.len(), 3);

        // Should preserve input order
        assert_eq!(group_infos[0].mls_group_id, group_id2);
        assert_eq!(group_infos[1].mls_group_id, group_id1);
        assert_eq!(group_infos[2].mls_group_id, group_id3);
    }
}
