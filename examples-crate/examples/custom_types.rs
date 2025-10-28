//! Custom serde types example.
//!
//! Demonstrates using any custom types that implement Serialize/Deserialize.
//! Run with: cargo run --example custom_types

use rpc_codec_json::JsonCodec;
use rpc_examples::{Preferences, Role, Transaction, TransactionResult, User, UserMetadata};
use rpc_server::rpc;
use rpc_transport_inprocess::InProcessTransport;

// Define RPC interface using custom types
rpc! {
    extern "Rust" {
        fn create_user(username: String, email: String) -> rpc_examples::User;
        fn get_user(id: u64) -> Option<rpc_examples::User>;
        fn update_preferences(user_id: u64, prefs: rpc_examples::Preferences) -> rpc_examples::User;
        fn process_transaction(tx: rpc_examples::Transaction) -> rpc_examples::TransactionResult;
        fn list_users_by_role(role: rpc_examples::Role) -> Vec<rpc_examples::User>;
    }
}

// Server implementation
struct UserService {
    users: std::sync::Arc<tokio::sync::Mutex<Vec<User>>>,
}

impl server::Server for UserService {
    async fn create_user(&self, username: String, email: String) -> User {
        println!("[Server] Creating user: {}", username);

        let mut users = self.users.lock().await;
        let id = users.len() as u64 + 1;

        let user = User {
            id,
            username,
            email,
            roles: vec![Role::User],
            metadata: UserMetadata {
                created_at: 1234567890,
                last_login: None,
                preferences: Preferences {
                    theme: "dark".to_string(),
                    language: "en".to_string(),
                    notifications_enabled: true,
                },
            },
        };

        users.push(user.clone());
        user
    }

    async fn get_user(&self, id: u64) -> Option<User> {
        println!("[Server] Fetching user {}", id);
        let users = self.users.lock().await;
        users.iter().find(|u| u.id == id).cloned()
    }

    async fn update_preferences(&self, user_id: u64, prefs: Preferences) -> User {
        println!("[Server] Updating preferences for user {}", user_id);
        let mut users = self.users.lock().await;

        let user = users.iter_mut().find(|u| u.id == user_id).unwrap();
        user.metadata.preferences = prefs;
        user.clone()
    }

    async fn process_transaction(&self, tx: Transaction) -> TransactionResult {
        println!(
            "[Server] Processing transaction: {} -> {} ({} {})",
            tx.from, tx.to, tx.amount, tx.currency
        );

        if tx.amount <= 0.0 {
            return TransactionResult {
                success: false,
                transaction_id: None,
                balance: 1000.0,
                error: Some("Invalid amount".to_string()),
            };
        }

        TransactionResult {
            success: true,
            transaction_id: Some(format!("tx_{}", tx.timestamp)),
            balance: 1000.0 - tx.amount,
            error: None,
        }
    }

    async fn list_users_by_role(&self, role: Role) -> Vec<User> {
        println!("[Server] Listing users with role: {:?}", role);
        let users = self.users.lock().await;
        users
            .iter()
            .filter(|u| u.roles.contains(&role))
            .cloned()
            .collect()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Custom Serde Types Example\n");
    println!("This demonstrates using complex custom types with RPC.\n");

    let (client_transport, server_transport) = InProcessTransport::pair();

    let users = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let service = UserService {
        users: users.clone(),
    };

    let server_handle = tokio::spawn(async move {
        let codec = JsonCodec;
        if let Err(e) = server::serve(service, server_transport, codec).await {
            eprintln!("[Server] Error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let codec = JsonCodec;
    let client = client::Client::new(client_transport, codec);

    println!("=== Creating users ===\n");

    let alice = client
        .create_user("alice".to_string(), "alice@example.com".to_string())
        .await?;
    println!("[Client] Created user: {:?}\n", alice);

    let bob = client
        .create_user("bob".to_string(), "bob@example.com".to_string())
        .await?;
    println!("[Client] Created user: {:?}\n", bob);

    println!("=== Fetching user ===\n");

    let user = client.get_user(1).await?;
    println!("[Client] Fetched user: {:?}\n", user);

    println!("=== Updating preferences ===\n");

    let new_prefs = Preferences {
        theme: "light".to_string(),
        language: "es".to_string(),
        notifications_enabled: false,
    };

    let updated_user = client.update_preferences(1, new_prefs).await?;
    println!("[Client] Updated user: {:?}\n", updated_user);

    println!("=== Processing transaction ===\n");

    let tx = Transaction {
        from: 1,
        to: 2,
        amount: 50.0,
        currency: "USD".to_string(),
        timestamp: 1234567890,
    };

    let result = client.process_transaction(tx).await?;
    println!("[Client] Transaction result: {:?}\n", result);

    println!("=== Listing users by role ===\n");

    let users: Vec<User> = client.list_users_by_role(Role::User).await?;
    println!("[Client] Found {} users with User role\n", users.len());

    println!("=== Summary ===");
    println!("Successfully used these custom types over RPC:");
    println!("- User (complex struct with nested types)");
    println!("- Role (enum)");
    println!("- UserMetadata (struct with Option fields)");
    println!("- Preferences (simple struct)");
    println!("- Transaction (input struct)");
    println!("- TransactionResult (output struct)");
    println!("\nAny type that implements Serialize + Deserialize works!");

    println!("\nDone!");

    server_handle.abort();

    Ok(())
}
