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
    use pact_consumer::mock_server::StartMockServerAsync;
    use pact_consumer::prelude::*;
    use pact_models::generators;
    use pact_models::prelude::*;
    use pact_models::v4::message_parts::MessageContents;

    use bytes::Bytes;
    use maplit::hashmap;
    use serde_json::json;

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
                    ..MessageContents::default()
                };
                i.request_contents(&m1);

                let m2 = MessageContents {
                    contents: OptionalBody::Present(Bytes::from("response one"), None, None),
                    metadata: hashmap! {
                        "response-key".to_string() => json!("response-value")
                    },
                    ..MessageContents::default()
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
            .synchronous_message_interaction(
                "a sync message with multiple responses",
                |mut i| async move {
                    let m1 = MessageContents {
                        contents: OptionalBody::Present(Bytes::from("request data"), None, None),
                        metadata: hashmap! {
                            "Origin".to_string() => json!("Some Text"),
                            "TagData".to_string() => json!({"ID": "sjhdjkshsdjh", "weight": 100.5})
                        },
                        ..MessageContents::default()
                    };
                    i.request_contents(&m1);

                    let m2 = MessageContents {
                        contents: OptionalBody::Present(Bytes::from("first response"), None, None),
                        ..MessageContents::default()
                    };
                    i.response_contents(&m2);

                    let m3 = MessageContents {
                        contents: OptionalBody::Present(Bytes::from("second response"), None, None),
                        ..MessageContents::default()
                    };
                    i.response_contents(&m3);

                    i
                },
            )
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
        assert!(response.contains(r#"request-metadata: {"Origin":"Some Text","TagData":{"ID":"sjhdjkshsdjh","weight":100.5}}"#)
            || response.contains(r#"request-metadata: {"TagData":{"ID":"sjhdjkshsdjh","weight":100.5},"Origin":"Some Text"}"#));
        assert!(response.contains("response-1-content:"));
        assert!(response.contains("first response"));
        assert!(response.contains("response-2-content:"));
        assert!(response.contains("second response"));
    }

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
    async fn test_sync_message_with_generators() {
        let mut builder = PactBuilderAsync::new_v4("SyncMessageConsumer", "SyncMessageProvider");

        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction("a sync message with generators", |mut i| async move {
                let m1 = MessageContents {
                    contents: OptionalBody::Present(
                        Bytes::from(r#"{"one":"a","two":"b"}"#),
                        None,
                        None,
                    ),
                    generators: generators! {
                        "BODY" => {
                            "$.one" => Generator::RandomInt(0, 1000)
                        }
                    },
                    ..MessageContents::default()
                };
                i.request_contents(&m1);

                let m2 = MessageContents {
                    contents: OptionalBody::Present(
                        Bytes::from(r#"{"one":"a","two":"b"}"#),
                        None,
                        None,
                    ),
                    generators: generators! {
                        "BODY" => {
                            "$.one" => Generator::RandomInt(0, 1000)
                        }
                    },
                    ..MessageContents::default()
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

        let response = client.send(r#"{"one":"a","two":"b"}"#).await.unwrap();

        let lines: Vec<&str> = response.lines().collect();
        let request_line = lines
            .iter()
            .find(|l| l.starts_with("request-content:"))
            .unwrap_or(&"");
        let request_value = request_line
            .strip_prefix("request-content:")
            .unwrap_or("")
            .trim()
            .to_string();
        let request_json: serde_json::Value = serde_json::from_str(&request_value).unwrap();
        assert!(
            request_json["one"].is_i64(),
            "request $.one should be integer, got: {}",
            request_json["one"]
        );
        assert!(
            request_json["one"].as_i64().unwrap() >= 0
                && request_json["one"].as_i64().unwrap() <= 1000
        );
        assert_eq!(request_json["two"], json!("b"));

        let response_line = lines
            .iter()
            .find(|l| l.starts_with("response-1-content:"))
            .unwrap_or(&"");
        let response_value = response_line
            .strip_prefix("response-1-content:")
            .unwrap_or("")
            .trim()
            .to_string();
        let response_json: serde_json::Value = serde_json::from_str(&response_value).unwrap();
        assert!(
            response_json["one"].is_i64(),
            "response $.one should be integer, got: {}",
            response_json["one"]
        );
    }

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
    async fn test_sync_message_metadata_generators() {
        let mut builder = PactBuilderAsync::new_v4("SyncMessageConsumer", "SyncMessageProvider");

        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction(
                "a sync message with metadata generators",
                |mut i| async move {
                    let m1 = MessageContents {
                        contents: OptionalBody::Present(Bytes::from("test"), None, None),
                        metadata: hashmap! {
                            "ID".to_string() => json!("sjhdjkshsdjh"),
                            "weight".to_string() => json!(100.5)
                        },
                        generators: generators! {
                            "METADATA" => {
                                "ID" => Generator::RandomInt(0, 1000)
                            }
                        },
                        ..MessageContents::default()
                    };
                    i.request_contents(&m1);

                    let m2 = MessageContents {
                        contents: OptionalBody::Present(Bytes::from("ack"), None, None),
                        metadata: hashmap! {
                            "ID".to_string() => json!("sjhdjkshsdjh"),
                            "weight".to_string() => json!(100.5)
                        },
                        generators: generators! {
                            "METADATA" => {
                                "ID" => Generator::RandomInt(0, 1000)
                            }
                        },
                        ..MessageContents::default()
                    };
                    i.response_contents(&m2);

                    i
                },
            )
            .await
            .start_mock_server_async(Some("sync-message-test/transport/tcp"), None)
            .await;

        let client = SyncMessageClient::new(
            mock_server.url().host_str().unwrap().to_string(),
            mock_server.url().port().unwrap(),
        );

        let response = client.send("test").await.unwrap();
        let lines: Vec<&str> = response.lines().collect();

        let metadata_line = lines
            .iter()
            .find(|l| l.starts_with("request-metadata:"))
            .unwrap_or(&"");
        let metadata_str = metadata_line
            .strip_prefix("request-metadata:")
            .unwrap_or("")
            .trim();
        let metadata: serde_json::Value = serde_json::from_str(metadata_str).unwrap();
        assert_eq!(metadata["weight"], json!(100.5));
        assert!(
            metadata["ID"].is_i64(),
            "ID should be integer, got: {}",
            metadata["ID"]
        );
    }

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
    async fn test_sync_message_with_random_string_generator() {
        let mut builder = PactBuilderAsync::new_v4("SyncMessageConsumer", "SyncMessageProvider");

        let mock_server = builder
            .using_plugin("sync-message-test", None)
            .await
            .synchronous_message_interaction(
                "a sync message with random string generator",
                |mut i| async move {
                    let m1 = MessageContents {
                        contents: OptionalBody::Present(
                            Bytes::from("random string request"),
                            None,
                            None,
                        ),
                        generators: generators! {
                            "BODY" => {
                                "$" => Generator::RandomString(15)
                            }
                        },
                        ..MessageContents::default()
                    };
                    i.request_contents(&m1);

                    let m2 = MessageContents {
                        contents: OptionalBody::Present(Bytes::from("random response"), None, None),
                        generators: generators! {
                            "BODY" => {
                                "$" => Generator::RandomString(15)
                            }
                        },
                        ..MessageContents::default()
                    };
                    i.response_contents(&m2);

                    i
                },
            )
            .await
            .start_mock_server_async(Some("sync-message-test/transport/tcp"), None)
            .await;

        let client = SyncMessageClient::new(
            mock_server.url().host_str().unwrap().to_string(),
            mock_server.url().port().unwrap(),
        );

        let response = client.send("random string request").await.unwrap();

        let lines: Vec<&str> = response.lines().collect();
        let request_content = lines
            .iter()
            .find(|l| l.starts_with("request-content:"))
            .unwrap_or(&"");
        let request_value = request_content
            .strip_prefix("request-content:")
            .unwrap_or("")
            .trim();
        assert_ne!(
            request_value, "random string request",
            "Request content should be generated"
        );
        assert_eq!(
            request_value.len(),
            15,
            "Request content should be 15 characters"
        );

        let response_content = lines
            .iter()
            .find(|l| l.starts_with("response-1-content:"))
            .unwrap_or(&"");
        let response_value = response_content
            .strip_prefix("response-1-content:")
            .unwrap_or("")
            .trim();
        assert_ne!(
            response_value, "random response",
            "Response content should be generated"
        );
        assert_eq!(
            response_value.len(),
            15,
            "Response content should be 15 characters"
        );
    }
}
