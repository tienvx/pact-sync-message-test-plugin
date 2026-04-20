use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct SyncMessageClient {
    host: String,
    port: u16,
}

impl SyncMessageClient {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }

    pub async fn send(&self, content: &str) -> anyhow::Result<String> {
        let addr = format!("{}:{}", self.host, self.port);
        let mut stream = TcpStream::connect(addr).await?;
        
        let request = format!("{}\n", content);
        stream.write_all(request.as_bytes()).await?;
        stream.flush().await?;
        
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use pact_consumer::prelude::*;
    use pact_consumer::mock_server::StartMockServerAsync;
    use pact_models::v4::message_parts::MessageContents;
    use pact_models::prelude::*;
    use pact_models::generators;

    use bytes::Bytes;
    use serde_json::json;
    use maplit::hashmap;

    use crate::SyncMessageClient;

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
    async fn test_sync_message_single_response() {
        let mut builder = PactBuilderAsync::new_v4("SyncMessageConsumer", "SyncMessageProvider");
        
        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction("a sync message request", |mut i| async move {
                let m1 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("hello world"), None, None),
                    metadata: hashmap! {
                        "request-key".to_string() => json!("request-value")
                    },
                    .. MessageContents::default()
                };
                i.request_contents(&m1);

                let m2 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("response one"), None, None),
                    metadata: hashmap! {
                        "response-key".to_string() => json!("response-value")
                    },
                    .. MessageContents::default()
                };
                i.response_contents(&m2);
                
                i
            })
            .await
            .start_mock_server_async(Some("sync-message-test/transport/tcp"), None)
            .await;

        let client = SyncMessageClient::new(
            mock_server.url().host_str().unwrap().to_string(),
            mock_server.url().port().unwrap(),
        );
        
        let response = client.send("hello world").await.unwrap();
        
        assert!(response.contains("request-content: hello world"));
        assert!(response.contains("request-content-type: text/plain"));
        assert!(response.contains(r#"request-metadata: {"request-key":"request-value"}"#));
        assert!(response.contains("response-1-content: response one"));
        assert!(response.contains("response-1-content-type: text/plain"));
        assert!(response.contains(r#"response-1-metadata: {"response-key":"response-value"}"#));
    }

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
    async fn test_sync_message_multiple_responses() {
        let builder = PactBuilder::new_v4("SyncMessageConsumer", "SyncMessageProvider");
        
        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction("a sync message with multiple responses", |mut i| async move {
                //i.transport("tcp");

                let m1 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("request data"), None, None),
                    .. MessageContents::default()
                };
                i.request_contents(&m1);

                let m2 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("first response"), None, None),
                    .. MessageContents::default()
                };
                i.response_contents(&m2);

                let m3 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("second response"), None, None),
                    .. MessageContents::default()
                };
                i.response_contents(&m3);
                
                i
            })
            .await
            .start_mock_server_async(Some("sync-message-test/transport/tcp"), None)
            .await;
        
        let client = SyncMessageClient::new(
            mock_server.url().host_str().unwrap().to_string(),
            mock_server.url().port().unwrap(),
        );
        
        let response = client.send("request data").await.unwrap();
        
        assert!(response.contains("request-content:"));
        assert!(response.contains("request data"));
        assert!(response.contains("response-1-content:"));
        assert!(response.contains("first response"));
        assert!(response.contains("response-2-content:"));
        assert!(response.contains("second response"));
    }

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
    async fn test_sync_message_with_metadata() {
        let mut builder = PactBuilderAsync::new_v4("SyncMessageConsumer", "SyncMessageProvider");
        
        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction("a sync message with metadata", |mut i| async move {
                //i.transport("tcp");

                let m1 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("test message"), None, None),
                    metadata: hashmap! {
                        "test key".to_string() => json!("test value")
                    },
                    .. MessageContents::default()
                };
                i.request_contents(&m1);

                let m2 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("acknowledged"), None, None),
                    .. MessageContents::default()
                };
                i.response_contents(&m2);
                
                i
            })
            .await
            .start_mock_server_async(Some("sync-message-test/transport/tcp"), None).await;
        
        let client = SyncMessageClient::new(
            mock_server.url().host_str().unwrap().to_string(),
            mock_server.url().port().unwrap(),
        );
        
        let response = client.send("test message").await.unwrap();

        assert!(response.contains("request-content: test message"));
        assert!(response.contains("request-content-type: text/plain"));
        assert!(response.contains(r#"request-metadata: {"test key":"test value"}"#));
        assert!(response.contains("response-1-content: acknowledged"));
        assert!(response.contains("response-1-content-type: text/plain"));
    }

   #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
     async fn test_sync_message_with_generators() {
        let mut builder = PactBuilderAsync::new_v4("SyncMessageConsumer", "SyncMessageProvider");
        
        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction("a sync message with generators", |mut i| async move {
                let m1 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("generator test request"), None, None),
                    generators: generators! {
                        "BODY" => {
                            "$" => Generator::Uuid(None)
                        }
                    },
                    .. MessageContents::default()
                };
                i.request_contents(&m1);

                let m2 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("generated response"), None, None),
                    generators: generators! {
                        "BODY" => {
                            "$" => Generator::Uuid(None)
                        }
                    },
                    .. MessageContents::default()
                };
                i.response_contents(&m2);
                
                i
            })
            .await
            .start_mock_server_async(Some("sync-message-test/transport/tcp"), None).await;
        
        let client = SyncMessageClient::new(
            mock_server.url().host_str().unwrap().to_string(),
            mock_server.url().port().unwrap(),
        );
        
        let response = client.send("generator test request").await.unwrap();
        eprintln!("Response: {}", response);

        assert!(response.contains("request-content: generator test request"));
        assert!(response.contains("request-content-type:"));
        
        let lines: Vec<&str> = response.lines().collect();
        let response_content = lines.iter().find(|l| l.starts_with("response-1-content:")).unwrap_or(&"");
        let response_value = response_content.strip_prefix("response-1-content:").unwrap_or("").trim();
        
        assert_ne!(response_value, "generated response", "Response content should be generated, not the fixed string");
        
        let uuid_pattern = regex::Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap();
        assert!(uuid_pattern.is_match(response_value), "Response content should be a valid UUID, got: {}", response_value);
    }

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
    async fn test_sync_message_with_random_string_generator() {
        let mut builder = PactBuilderAsync::new_v4("SyncMessageConsumer", "SyncMessageProvider");
        
        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction("a sync message with random string generator", |mut i| async move {
                let m1 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("random string request"), None, None),
                    generators: generators! {
                        "BODY" => {
                            "$" => Generator::RandomString(15)
                        }
                    },
                    .. MessageContents::default()
                };
                i.request_contents(&m1);

                let m2 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("random response"), None, None),
                    generators: generators! {
                        "BODY" => {
                            "$" => Generator::RandomString(15)
                        }
                    },
                    .. MessageContents::default()
                };
                i.response_contents(&m2);
                
                i
            })
            .await
            .start_mock_server_async(Some("sync-message-test/transport/tcp"), None).await;
        
        let client = SyncMessageClient::new(
            mock_server.url().host_str().unwrap().to_string(),
            mock_server.url().port().unwrap(),
        );
        
        let response = client.send("random string request").await.unwrap();
        eprintln!("Response: {}", response);

        assert!(response.contains("request-content:"));
        
        let lines: Vec<&str> = response.lines().collect();
        let response_content = lines.iter().find(|l| l.starts_with("response-1-content:")).unwrap_or(&"");
        let response_value = response_content.strip_prefix("response-1-content:").unwrap_or("").trim();
        
        assert_ne!(response_value, "random response", "Response content should be generated, not the fixed string");
        assert_eq!(response_value.len(), 15, "Response content should be 15 characters long");
        assert!(response_value.chars().all(|c| c.is_ascii_alphanumeric()), "Response content should be alphanumeric");
    }
}
