# Pact Sync Message Test Plugin

A Pact V4 plugin for testing synchronous message interactions over TCP transport.

## Overview

This plugin provides:
- **TCP Transport**: Listen for synchronous message requests on a TCP socket
- **Content Type**: `application/test` for test message format
- **Synchronous Messages**: Support for request-response message patterns

## Consumer Side Behavior

When a sync message request matches a registered interaction:
1. The mock server receives the request via TCP
2. It sends back a formatted response containing:
   - Original request content (with generators applied)
   - Request content type
   - Request metadata
   - All expected responses (with generators applied)
   - Response content types
   - Response metadata

### Response Format

```
request-content: [request content (applied generators)\n]
request-content-type: [request content type\n]
request-metadata: [request metadata\n]
response-1-content: [response 1 content (applied generators)\n]
response-1-content-type: [response 1 content type\n]
response-1-metadata: [response 1 metadata\n]
response-2-content: [response 2 content (applied generators)\n]
response-2-content-type: [response 2 content type\n]
response-2-metadata: [response 2 metadata\n]
...
```

## Provider Side (Not Implemented)

The provider side is not implemented in this plugin. The intended behavior would be:
1. Mock server sends request (with generators applied) to provider via TCP
2. Provider sends response back
3. Response is verified using matchers against expected responses

## Building

```bash
cargo build --release
```

The binary will be available at `target/release/pact-sync-message-test-plugin`.

## Usage

### Consumer Test Example

See `examples/consumer-test/src/lib.rs` for a complete example of how to use this plugin in consumer tests.

```rust
use pact_consumer::prelude::*;
use pact_models::prelude::*;
use serde_json::json;

#[tokio::test]
async fn test_sync_message() {
    let mut builder = PactBuilder::new_v4("Consumer", "Provider")
        .using_plugin("sync-message-test", None)
        .await;
    
    builder
        .interaction("a sync message request", "", |mut i| async move {
            i.transport("tcp");
            
            i.request
                .contents(ContentType::from("application/test"), json!({
                    "content": "hello world"
                }))
                .await;
            
            i.response
                .contents(ContentType::from("application/test"), json!({
                    "content": "response"
                }))
                .await;
            
            i
        })
        .await;
    
    let mock_server = builder.start_mock_server(Some("tcp"), None);
    
    // Your test code here
    // Connect to mock_server.url() via TCP
}
```

### Note on Running Tests

The example tests demonstrate the API usage but may encounter runtime issues when using the pact_consumer mock server with plugins. This is a known limitation when using plugins with synchronous message interactions.

For development and testing, you can:

1. **Run the plugin directly** to test the TCP transport:
```bash
# Build and run
cargo run --release
```

The plugin will output JSON with the port and server key:
```json
{"port":12345, "serverKey":"uuid-here"}
```

2. **Test manually** using a TCP client:
```bash
# In one terminal, start the plugin
cargo run --release

# In another terminal, send a test message
echo "test message" | nc localhost <port>
```

3. **Install the plugin** for use with Pact:
```bash
mkdir -p ~/.pact/plugins/sync-message-test-0.1.0
cp target/release/pact-sync-message-test-plugin ~/.pact/plugins/sync-message-test-0.1.0/
cp pact-plugin.json ~/.pact/plugins/sync-message-test-0.1.0/
```

## Configuration

The plugin registers the following catalogue entries:
- Transport: `tcp`
- Interaction: `synchronous-messages` with content type `application/test`

## References

- [Pact V4 Specification](https://github.com/pact-foundation/pact-specification)
- [Pact Plugin Interface](https://github.com/pact-foundation/pact-plugins)
- [Pact CSV Plugin](https://github.com/pact-foundation/pact-plugins/tree/main/plugins/csv)
- [Pact Protobuf Plugin](https://github.com/pactflow/pact-protobuf-plugin)
