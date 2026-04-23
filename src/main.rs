use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use log::{debug, info};
use maplit::hashmap;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{transport::Server, Response, Status};
use uuid::Uuid;

use pact_models::generators::NoopVariantMatcher;

fn generator_context() -> (
    std::collections::HashMap<&'static str, Value>,
    Box<dyn pact_models::generators::VariantMatcher + Send + Sync>,
) {
    (
        std::collections::HashMap::new(),
        Box::new(NoopVariantMatcher),
    )
}

fn value_to_hashmap(v: &Value) -> HashMap<String, Value> {
    v.as_object()
        .cloned()
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

use crate::proto::catalogue_entry::EntryType;
use crate::proto::pact_plugin_server::{PactPlugin, PactPluginServer};

mod proto;

#[derive(Debug, Clone)]
pub struct SyncMessageConfig {
    pub request_content: Vec<u8>,
    pub request_content_type: String,
    pub request_metadata_with_generators: HashMap<String, Value>,
    pub request_generated_content: String,
    pub responses: Vec<SyncMessageResponse>,
}

#[derive(Debug, Clone)]
pub struct SyncMessageResponse {
    pub content: Vec<u8>,
    pub content_type: String,
    pub metadata_with_generators: HashMap<String, Value>,
    pub generated_content: String,
}

#[derive(Debug, Default)]
pub struct SyncMessagePactPlugin {
    interactions: Arc<Mutex<HashMap<String, SyncMessageConfig>>>,
}

#[tonic::async_trait]
impl PactPlugin for SyncMessagePactPlugin {
    async fn init_plugin(
        &self,
        request: tonic::Request<proto::InitPluginRequest>,
    ) -> Result<tonic::Response<proto::InitPluginResponse>, tonic::Status> {
        let message = request.get_ref();
        debug!(
            "Init request from {}/{}",
            message.implementation, message.version
        );

        Ok(Response::new(proto::InitPluginResponse {
            catalogue: vec![
                proto::CatalogueEntry {
                    r#type: EntryType::Transport as i32,
                    key: "tcp".to_string(),
                    values: hashmap! {},
                },
                proto::CatalogueEntry {
                    r#type: EntryType::Interaction as i32,
                    key: "synchronous-messages".to_string(),
                    values: hashmap! {
                        "content-types".to_string() => "application/test".to_string()
                    },
                },
            ],
        }))
    }

    async fn update_catalogue(
        &self,
        _request: tonic::Request<proto::Catalogue>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        debug!("Update catalogue request, ignoring");
        Ok(Response::new(()))
    }

    async fn compare_contents(
        &self,
        request: tonic::Request<proto::CompareContentsRequest>,
    ) -> Result<tonic::Response<proto::CompareContentsResponse>, tonic::Status> {
        let req = request.get_ref();
        debug!("compare_contents request - {:?}", req);

        let exp = req.expected.as_ref().and_then(|e| e.content.as_ref());
        let act = req.actual.as_ref().and_then(|a| a.content.as_ref());

        let mismatch = match (exp, act) {
            (Some(e), Some(a)) if e != a => proto::ContentMismatch {
                expected: Some(e.clone()),
                actual: Some(a.clone()),
                mismatch: "Content mismatch".into(),
                ..Default::default()
            },
            (None, Some(a)) => proto::ContentMismatch {
                expected: None,
                actual: Some(a.clone()),
                mismatch: format!("Expected no content, but got {} bytes", a.len()),
                ..Default::default()
            },
            (Some(e), None) => proto::ContentMismatch {
                expected: Some(e.clone()),
                actual: None,
                mismatch: "Expected content, but did not get any".into(),
                ..Default::default()
            },
            _ => return Ok(Response::new(proto::CompareContentsResponse::default())),
        };

        Ok(Response::new(proto::CompareContentsResponse {
            results: hashmap! {
                String::new() => proto::ContentMismatches {
                    mismatches: vec![mismatch],
                }
            },
            ..Default::default()
        }))
    }

    async fn configure_interaction(
        &self,
        request: tonic::Request<proto::ConfigureInteractionRequest>,
    ) -> Result<tonic::Response<proto::ConfigureInteractionResponse>, tonic::Status> {
        debug!(
            "Received configure_interaction request for '{}'",
            request.get_ref().content_type
        );

        let contents_config = request.get_ref().contents_config.as_ref();

        // Extract metadata from contentsConfig - it may be nested under "metadata" key or at the top level
        let message_metadata = if let Some(config) = contents_config {
            if let Some(metadata) = config.fields.get("metadata") {
                // Metadata is nested under "metadata" key
                metadata.kind.as_ref().and_then(|k| match k {
                    prost_types::value::Kind::StructValue(s) => Some(s.clone()),
                    _ => None,
                })
            } else {
                // Metadata is the entire contentsConfig
                Some(config.clone())
            }
        } else {
            None
        };

        let interactions = vec![proto::InteractionResponse {
            contents: None,
            rules: hashmap! {},
            generators: hashmap! {},
            message_metadata,
            plugin_configuration: None,
            interaction_markup: String::default(),
            interaction_markup_type: 0,
            part_name: "request".to_string(),
        }];

        Ok(Response::new(proto::ConfigureInteractionResponse {
            error: String::default(),
            interaction: interactions,
            plugin_configuration: None,
        }))
    }

    async fn generate_content(
        &self,
        request: tonic::Request<proto::GenerateContentRequest>,
    ) -> Result<tonic::Response<proto::GenerateContentResponse>, tonic::Status> {
        debug!("Received generate_content request");
        let req = request.get_ref();

        let contents = req.contents.as_ref();
        let body = contents.map(|c| proto::Body {
            content_type: c.content_type.clone(),
            content: c.content.clone(),
            content_type_hint: c.content_type_hint,
        });

        Ok(Response::new(proto::GenerateContentResponse {
            contents: body,
        }))
    }

    async fn start_mock_server(
        &self,
        request: tonic::Request<proto::StartMockServerRequest>,
    ) -> Result<tonic::Response<proto::StartMockServerResponse>, tonic::Status> {
        debug!("Received start_mock_server request");
        let req = request.get_ref();

        let host = if req.host_interface.is_empty() {
            "127.0.0.1"
        } else {
            req.host_interface.as_str()
        };
        let port = req.port;

        let addr: SocketAddr = format!("{}:{}", host, port).parse().map_err(|e| {
            Status::invalid_argument(format!(
                "Invalid address: {} - host: {}, port: {}",
                e, host, port
            ))
        })?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| Status::internal(format!("Failed to bind: {}", e)))?;

        let address = listener
            .local_addr()
            .map_err(|e| Status::internal(format!("Failed to get local address: {}", e)))?;

        let server_key = Uuid::new_v4().to_string();
        let server_key_for_spawn = server_key.clone();
        let interactions = self.interactions.clone();
        let pact_json = req.pact.clone();

        tokio::spawn(async move {
            run_mock_server(listener, server_key_for_spawn, interactions, pact_json).await;
        });

        Ok(Response::new(proto::StartMockServerResponse {
            response: Some(proto::start_mock_server_response::Response::Details(
                proto::MockServerDetails {
                    key: server_key,
                    port: address.port() as u32,
                    address: format!("tcp://{}:{}", address.ip(), address.port()),
                },
            )),
        }))
    }

    async fn shutdown_mock_server(
        &self,
        request: tonic::Request<proto::ShutdownMockServerRequest>,
    ) -> Result<tonic::Response<proto::ShutdownMockServerResponse>, tonic::Status> {
        debug!(
            "Received shutdown_mock_server request for server: {}",
            request.get_ref().server_key
        );

        Ok(Response::new(proto::ShutdownMockServerResponse {
            ok: true,
            results: vec![],
        }))
    }

    async fn get_mock_server_results(
        &self,
        request: tonic::Request<proto::MockServerRequest>,
    ) -> Result<tonic::Response<proto::MockServerResults>, tonic::Status> {
        debug!(
            "Received get_mock_server_results request for server: {}",
            request.get_ref().server_key
        );

        Ok(Response::new(proto::MockServerResults {
            ok: true,
            results: vec![],
        }))
    }

    async fn prepare_interaction_for_verification(
        &self,
        _request: tonic::Request<proto::VerificationPreparationRequest>,
    ) -> Result<tonic::Response<proto::VerificationPreparationResponse>, tonic::Status> {
        Ok(Response::new(proto::VerificationPreparationResponse {
            response: None,
        }))
    }

    async fn verify_interaction(
        &self,
        _request: tonic::Request<proto::VerifyInteractionRequest>,
    ) -> Result<tonic::Response<proto::VerifyInteractionResponse>, tonic::Status> {
        Ok(Response::new(proto::VerifyInteractionResponse {
            response: None,
        }))
    }
}

async fn run_mock_server(
    listener: TcpListener,
    server_key: String,
    interactions: Arc<Mutex<HashMap<String, SyncMessageConfig>>>,
    pact_json: String,
) {
    info!("Mock server started with key: {}", server_key);

    let pact: Value = serde_json::from_str(&pact_json).unwrap_or_else(|_| serde_json::json!({}));

    let interactions_map = extract_interactions(&pact);
    interactions.lock().await.extend(interactions_map);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!("Connection from: {}", addr);
                let interactions = interactions.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, interactions).await {
                        debug!("Error handling client: {}", e);
                    }
                });
            }
            Err(e) => {
                debug!("Error accepting connection: {}", e);
                break;
            }
        }
    }
}

fn set_json_path(value: &mut Value, path: &str, new_value: &Value) {
    let parts: Vec<&str> = path
        .trim_start_matches('$')
        .trim_start_matches('.')
        .split('.')
        .collect();
    match value {
        Value::Object(map) => {
            if parts.len() == 1 {
                map.insert(parts[0].to_string(), new_value.clone());
            } else if let Some(first) = parts.first() {
                if let Some(child) = map.get_mut(&first.to_string()) {
                    set_json_path(child, &parts[1..].join("."), new_value);
                }
            }
        }
        Value::Array(arr) => {
            if let Some(first) = parts.first() {
                if let Ok(idx) = first.parse::<usize>() {
                    if idx < arr.len() {
                        if parts.len() == 1 {
                            arr[idx] = new_value.clone();
                        } else {
                            set_json_path(&mut arr[idx], &parts[1..].join("."), new_value);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn apply_generators_to_body(content: &str, generators: &Value) -> String {
    use pact_models::generators::{GenerateValue, Generator};

    let body_generators = generators.get("body").and_then(|v| v.as_object());
    let Some(body_generators) = body_generators else {
        return content.to_string();
    };

    // Try JSON path-based generators first
    let mut parsed: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => {
            // Non-JSON: try whole-body generator
            return generate_first_match(content, body_generators).unwrap_or(content.to_string());
        }
    };

    let content_str = content.to_string();
    for (path_key, gen_def) in body_generators {
        let gen_type = gen_def.get("type").and_then(|v| v.as_str());
        let Some((gen_type, gen_map)) = gen_type.zip(gen_def.as_object()) else { continue };
        let Some(generator) = Generator::from_map(gen_type, gen_map) else { continue };

        let (context, matcher) = generator_context();
        let Ok(result) = generator.generate_value(&content_str, &context, &matcher) else { continue };

        let result_val: Value = serde_json::from_str(&result).unwrap_or(Value::String(result));
        let key = path_key.strip_prefix('$').unwrap_or(path_key).trim_start_matches('.');
        set_json_path(&mut parsed, key, &result_val);
    }

    serde_json::to_string(&parsed).unwrap_or_else(|_| content.to_string())
}

fn generate_first_match(content: &str, gens: &serde_json::Map<String, Value>) -> Option<String> {
    use pact_models::generators::{GenerateValue, Generator};
    let content_str = content.to_string();
    gens.values().find_map(|gen_def| {
        let gen_type = gen_def.get("type").and_then(|v| v.as_str())?;
        let gen_map = gen_def.as_object()?;
        let generator = Generator::from_map(gen_type, gen_map)?;
        let (context, matcher) = generator_context();
        generator.generate_value(&content_str, &context, &matcher).ok()
    })
}

fn apply_generators_to_metadata(metadata: &Value, generators: &Value) -> Value {
    use pact_models::generators::GenerateValue;

    let metadata_generators = generators.get("metadata").and_then(|v| v.as_object());
    let Some(metadata_generators) = metadata_generators else { return metadata.clone() };
    let Some(metadata_obj) = metadata.as_object() else { return metadata.clone() };

    let mut result = serde_json::Map::new();
    for (key, value) in metadata_obj {
        let generated = metadata_generators.get(key).and_then(|gen_def| {
            let gen_type = gen_def.get("type").and_then(|v| v.as_str())?;
            let gen_map = gen_def.as_object()?;
            let generator = pact_models::generators::Generator::from_map(gen_type, gen_map)?;
            let (context, matcher) = generator_context();
            generator.generate_value(&value.to_string(), &context, &matcher).ok()
        });

        result.insert(key.clone(), match generated {
            Some(s) => parse_generated_value(&s),
            None => value.clone(),
        });
    }
    Value::Object(result)
}

fn parse_generated_value(s: &str) -> Value {
    serde_json::from_str(s).ok()
        .or_else(|| s.parse::<i64>().ok().map(|n| Value::Number(n.into())))
        .or_else(|| s.parse::<f64>().ok().and_then(|n| serde_json::Number::from_f64(n).map(Value::Number)))
        .unwrap_or(Value::String(s.to_string()))
}

fn body_matches_structure(stored: &Value, received: &Value) -> bool {
    match (stored, received) {
        (Value::Object(stored_map), Value::Object(received_map)) => {
            if stored_map.len() != received_map.len() {
                return false;
            }
            for (key, stored_val) in stored_map {
                // Skip generator matcher keys when comparing
                if key.starts_with("pact:") {
                    continue;
                }
                let Some(received_val) = received_map.get(key) else {
                    return false;
                };
                if !body_matches_structure(stored_val, received_val) {
                    return false;
                }
            }
            true
        }
        (Value::Array(stored_arr), Value::Array(received_arr)) => {
            if stored_arr.len() != received_arr.len() {
                return false;
            }
            for (stored_val, received_val) in stored_arr.iter().zip(received_arr.iter()) {
                if !body_matches_structure(stored_val, received_val) {
                    return false;
                }
            }
            true
        }
        _ => true, // Non-object/non-array values are considered matching if they reach here
    }
}

fn extract_interactions(pact: &Value) -> HashMap<String, SyncMessageConfig> {
    let mut interactions = HashMap::new();

    let Some(interactions_arr) = pact.get("interactions").and_then(|v| v.as_array()) else {
        return interactions;
    };

    for interaction in interactions_arr {
        if interaction.get("type").and_then(|v| v.as_str()) != Some("Synchronous/Messages") {
            continue;
        }

        let key = interaction.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        info!("Processing interaction: {}", key);

        let request = interaction.get("request");
        let (request_content, request_content_type) = extract_content_and_type(request);
        let request_metadata = request.and_then(|r| r.get("metadata")).cloned().unwrap_or(Value::Null);
        let request_generators = request.and_then(|r| r.get("generators")).cloned().unwrap_or(Value::Null);
        info!("Request generators: {:?}", request_generators);

        let request_generated_content = apply_generators_to_body(
            &String::from_utf8_lossy(&request_content),
            &request_generators,
        );
        info!("Request generated content: {}", request_generated_content);

        let request_metadata_with_generators =
            apply_generators_to_metadata(&request_metadata, &request_generators);
        info!("Request metadata with generators: {:?}", request_metadata_with_generators);

        let responses = interaction.get("response").and_then(|v| v.as_array())
            .map(|response_arr| response_arr.iter().map(|resp| {
                let (content, content_type) = extract_content_and_type(Some(resp));
                let generators = resp.get("generators").cloned().unwrap_or(Value::Null);
                let response_metadata = resp.get("metadata").cloned().unwrap_or(Value::Null);

                SyncMessageResponse {
                    metadata_with_generators: value_to_hashmap(
                        &apply_generators_to_metadata(&response_metadata, &generators),
                    ),
                    generated_content: apply_generators_to_body(
                        &String::from_utf8_lossy(&content),
                        &generators,
                    ),
                    content,
                    content_type,
                }
            }).collect())
            .unwrap_or_default();

        interactions.insert(key, SyncMessageConfig {
            request_metadata_with_generators: value_to_hashmap(&request_metadata_with_generators),
            request_generated_content,
            responses,
            request_content,
            request_content_type,
        });
    }

    interactions
}

fn extract_content_and_type(part: Option<&Value>) -> (Vec<u8>, String) {
    let contents = part.and_then(|p| p.get("contents"));
    let content = contents.and_then(|c| c.get("content")).map(|v| {
        v.as_str().map(|s| s.as_bytes().to_vec())
            .or_else(|| v.as_object().map(|o| serde_json::to_string(o).unwrap_or_default().into_bytes()))
            .or_else(|| v.as_array().map(|a| serde_json::to_string(a).unwrap_or_default().into_bytes()))
            .unwrap_or_default()
    }).unwrap_or_default();

    let content_type = contents.and_then(|c| c.get("contentType"))
        .and_then(|v| v.as_str())
        .unwrap_or("application/test")
        .to_string();

    (content, content_type)
}

async fn handle_client(
    mut stream: TcpStream,
    interactions: Arc<Mutex<HashMap<String, SyncMessageConfig>>>,
) -> Result<(), anyhow::Error> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte).await {
            Ok(0) => return Ok(()),
            Ok(_) => {
                buffer.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            Err(e) => return Err(e.into()),
        }
    }

    let received_str = String::from_utf8_lossy(&buffer);
    debug!("Received: {}", received_str);

    let interactions_map = interactions.lock().await;

    let received_bytes = received_str.trim().as_bytes().to_vec();

    // Try exact match first
    let matching_interaction = interactions_map
        .values()
        .find(|config| config.request_content == received_bytes)
        .or_else(|| {
            // If no exact match, try matching by structure (ignoring generator matchers)
            let received: Value = match serde_json::from_str(received_str.trim()) {
                Ok(v) => v,
                Err(_) => return None,
            };
            interactions_map.values().find(|config| {
                let stored: Value = match serde_json::from_slice(&config.request_content) {
                    Ok(v) => v,
                    Err(_) => return false,
                };
                body_matches_structure(&stored, &received)
            })
        });

    // If still no match, try matching the received body content against the pact JSON
    let matching_interaction = matching_interaction.or_else(|| {
        let received_str_trimmed = received_str.trim();
        if let Ok(received) = serde_json::from_str::<Value>(received_str_trimmed) {
            interactions_map.values().find(|config| {
                let stored_str = String::from_utf8_lossy(&config.request_content);
                if let Ok(stored) = serde_json::from_str::<Value>(stored_str.as_ref()) {
                    let received_json = serde_json::to_string(&received).unwrap_or_default();
                    let stored_json = serde_json::to_string(&stored).unwrap_or_default();
                    received_json == stored_json
                } else {
                    let stored_str = stored_str.to_string();
                    received_str_trimmed == stored_str
                }
            })
        } else {
            None
        }
    });

    // If still no match and received body looks like incomplete JSON, try prefix match
    let matching_interaction = matching_interaction.or_else(|| {
        let received_trimmed = received_str.trim();
        if received_trimmed.starts_with('{') || received_trimmed.starts_with('[') {
            interactions_map.values().find(|config| {
                let stored_str = String::from_utf8_lossy(&config.request_content);
                stored_str.starts_with(received_trimmed)
            })
        } else {
            None
        }
    });

    if let Some(config) = matching_interaction {
        info!(
            "Found matching interaction, responses count: {}",
            config.responses.len()
        );
        let mut response = Vec::new();

        response
            .extend(format!("request-content: {}\n", config.request_generated_content).as_bytes());
        response
            .extend(format!("request-content-type: {}\n", config.request_content_type).as_bytes());

        if !config.request_metadata_with_generators.is_empty() {
            let metadata_json = serde_json::to_string(&config.request_metadata_with_generators)
                .unwrap_or_else(|_| "{}".to_string());
            response.extend(format!("request-metadata: {}\n", metadata_json).as_bytes());
        }

        // Send response content if available
        if !config.responses.is_empty() {
            info!(
                "Sending {} responses from pact JSON",
                config.responses.len()
            );
            for (i, resp) in config.responses.iter().enumerate() {
                response.extend(
                    format!("response-{}-content: {}\n", i + 1, resp.generated_content).as_bytes(),
                );
                response.extend(
                    format!("response-{}-content-type: {}\n", i + 1, resp.content_type).as_bytes(),
                );

                let response_metadata_str = serde_json::to_string(&resp.metadata_with_generators)
                    .unwrap_or_else(|_| "{}".to_string());
                response.extend(
                    format!("response-{}-metadata: {}\n", i + 1, response_metadata_str).as_bytes(),
                );
            }
        } else {
            // If no responses from pact JSON, echo the received body
            info!("No responses in pact JSON, echoing received body");
            let received_str_trimmed = received_str.trim().to_string();

            info!("Response content: {}", received_str_trimmed);
            response.extend(format!("response-1-content: {}\n", received_str_trimmed).as_bytes());
            if serde_json::from_str::<Value>(received_str.trim()).is_ok() {
                response.extend(b"response-1-content-type: application/json\n");
            } else {
                response.extend(b"response-1-content-type: text/plain\n");
            }
        }

        stream.write_all(&response).await?;
        stream.flush().await?;
    } else {
        debug!("No matching interaction found");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_level = env::var("PACT_LOGLEVEL")
        .or_else(|_| env::var("LOG_LEVEL"))
        .unwrap_or_else(|_| "INFO".to_string());

    std::fs::create_dir_all("./log")?;
    let file_appender = tracing_appender::rolling::daily("./log", "plugin.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_max_level(
            tracing_core::LevelFilter::from_str(&log_level)
                .unwrap_or(tracing_core::LevelFilter::INFO),
        )
        .with_writer(non_blocking)
        .with_thread_names(true)
        .with_ansi(false)
        .init();

    info!("Starting sync-message-test-plugin");
    let addr: SocketAddr = "0.0.0.0:0".parse()?;
    let listener = TcpListener::bind(addr).await?;
    let address = listener.local_addr()?;

    let server_key = Uuid::new_v4().to_string();
    println!(
        "{{\"port\":{}, \"serverKey\":\"{}\"}}",
        address.port(),
        server_key
    );
    let _ = std::io::stdout().flush();

    info!(
        "Plugin started on port {} with server_key: {}",
        address.port(),
        server_key
    );

    let plugin = SyncMessagePactPlugin::default();
    Server::builder()
        .add_service(PactPluginServer::new(plugin))
        .serve_with_incoming(TcpListenerStream::new(listener))
        .await?;

    Ok(())
}
