# Agent Bridge Platform

A Rust-based backend system designed to serve as an asynchronous communication bridge between a local TypeScript Agent Runtime and multiple web application clients via WebSockets.

## Structure

- **common**: Shared code between services
- **websocket-server**: Core WebSocket server using actix actors
- **web-server**: Web frontend server with user authentication

## Development

To build all crates:

```bash
cargo build
```

To run the WebSocket server:

```bash
cargo run -p websocket-server
```

To run the Web server:

```bash
cargo run -p web-server
```
