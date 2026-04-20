# Build and Test Commands

## Build the plugin
```bash
cargo build --release
```

## Install the plugin
```bash
mkdir -p ~/.pact/plugins/sync-message-test-0.1.0
cp target/release/pact-sync-message-test-plugin ~/.pact/plugins/sync-message-test-0.1.0/
cp pact-plugin.json ~/.pact/plugins/sync-message-test-0.1.0/
```

## Run the plugin
```bash
cargo run --release
```

## Run tests (consumer example)
Note: Tests may encounter runtime issues due to tokio constraints with plugins.
```bash
cd examples/consumer-test
PACT_DO_NOT_TRACK=true cargo test
```
