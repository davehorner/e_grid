# IPC Protocol Tests

This directory contains tests for the new E-Grid IPC protocol types and flows.

## How to Run

```
cargo test --test ipc_protocol_tests
cargo test --test ipc_protocol_fuzz_tests
cargo test --test ipc_protocol_integration_tests
```

## What is Tested
- Command and response struct construction
- Enum field correctness
- Serialization/deserialization roundtrips (bincode)
- Fuzz tests for invalid/unknown command types
- Integration tests for request/response flows

## Next Steps
- Begin full client/server codebase refactor to use only the new protocol types
