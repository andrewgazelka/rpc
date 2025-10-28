// Shared types for examples

use schema::Schema;
use serde::{Deserialize, Serialize};

// Types for codec_mixing example
#[derive(Debug, Clone, Serialize, Deserialize, Schema, PartialEq)]
pub struct Analysis {
    pub length: usize,
    pub word_count: usize,
    pub char_count: usize,
    pub uppercase_count: usize,
}

// Types for custom_types example
#[derive(Debug, Clone, Serialize, Deserialize, Schema, PartialEq)]
pub struct User {
    pub id: u64,
    pub username: String,
    pub email: String,
    pub roles: Vec<Role>,
    pub metadata: UserMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema, PartialEq)]
pub enum Role {
    Admin,
    Moderator,
    User,
    Guest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema, PartialEq)]
pub struct UserMetadata {
    pub created_at: u64,
    pub last_login: Option<u64>,
    pub preferences: Preferences,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema, PartialEq)]
pub struct Preferences {
    pub theme: String,
    pub language: String,
    pub notifications_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub struct Transaction {
    pub from: u64,
    pub to: u64,
    pub amount: f64,
    pub currency: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub struct TransactionResult {
    pub success: bool,
    pub transaction_id: Option<String>,
    pub balance: f64,
    pub error: Option<String>,
}
