use crate::{Account, Group, Whitenoise, WhitenoiseError};
use std::collections::HashMap;

#[derive(Clone)]
pub struct ScenarioContext {
    pub whitenoise: &'static Whitenoise,
    pub dev_relays: Vec<&'static str>,
    pub accounts: HashMap<String, Account>,
    pub groups: HashMap<String, Group>,
    pub messages_ids: HashMap<String, String>,
    pub tests_count: u32,
    pub tests_passed: u32,
}

impl ScenarioContext {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            whitenoise,
            dev_relays: vec!["ws://localhost:8080", "ws://localhost:7777"],
            accounts: HashMap::new(),
            groups: HashMap::new(),
            messages_ids: HashMap::new(),
            tests_count: 0,
            tests_passed: 0,
        }
    }

    pub fn add_account(&mut self, name: &str, account: Account) {
        self.accounts.insert(name.to_string(), account);
    }

    pub fn get_account(&self, name: &str) -> Result<&Account, WhitenoiseError> {
        self.accounts
            .get(name)
            .ok_or(WhitenoiseError::AccountNotFound)
    }

    pub fn add_group(&mut self, name: &str, group: Group) {
        self.groups.insert(name.to_string(), group);
    }

    pub fn get_group(&self, name: &str) -> Result<&Group, WhitenoiseError> {
        self.groups.get(name).ok_or(WhitenoiseError::GroupNotFound)
    }

    pub fn add_message_id(&mut self, name: &str, message_id: String) {
        self.messages_ids.insert(name.to_string(), message_id);
    }

    pub fn get_message_id(&self, message_id: &str) -> Result<&String, WhitenoiseError> {
        self.messages_ids.get(message_id).ok_or_else(|| {
            WhitenoiseError::Configuration(format!(
                "Message ID '{}' not found in context",
                message_id
            ))
        })
    }

    pub fn record_test(&mut self, passed: bool) {
        self.tests_count += 1;
        if passed {
            self.tests_passed += 1;
        }
    }
}
