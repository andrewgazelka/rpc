//! Test that types defined above the rpc! macro are visible in generated code.

use rpc_server::rpc;
use schema::Schema;
use serde::{Deserialize, Serialize};

// Define types ABOVE the macro
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Schema)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Schema)]
pub struct CreateUserRequest {
    pub name: String,
}

// Use the macro with unqualified type references
rpc! {
    extern "Rust" {
        fn create_user(req: CreateUserRequest) -> User;
        fn get_user(id: u64) -> Option<User>;
    }
}

// Server implementation
struct UserService;

impl server::AsyncService for UserService {
    async fn create_user(&self, req: CreateUserRequest) -> User {
        User {
            id: 1,
            name: req.name,
        }
    }

    async fn get_user(&self, id: u64) -> Option<User> {
        if id == 1 {
            Some(User {
                id: 1,
                name: "Alice".to_string(),
            })
        } else {
            None
        }
    }
}

#[tokio::test]
async fn test_local_types() {
    use rpc_codec_json::JsonCodec;
    use rpc_transport_inprocess::InProcessTransport;

    let (client_transport, server_transport) = InProcessTransport::pair();

    let service = UserService;

    let server_handle = tokio::spawn(async move {
        let codec = JsonCodec;
        if let Err(e) = server::serve(service, server_transport, codec).await {
            eprintln!("Server error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let codec = JsonCodec;
    let client = client::Client::new(client_transport, codec);

    // Test create_user
    let req = CreateUserRequest {
        name: "Bob".to_string(),
    };
    let user = client.create_user(req).await.unwrap();
    assert_eq!(user.id, 1);
    assert_eq!(user.name, "Bob");

    // Test get_user
    let user = client.get_user(1).await.unwrap();
    assert_eq!(
        user,
        Some(User {
            id: 1,
            name: "Alice".to_string(),
        })
    );

    let no_user = client.get_user(999).await.unwrap();
    assert_eq!(no_user, None);

    server_handle.abort();
}
