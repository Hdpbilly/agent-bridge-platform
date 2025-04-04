
# Agent Bridge Platform - Architecture & Development Guide

## Project Overview

The Agent Bridge Platform is a Rust-based backend system that provides an asynchronous communication bridge between a TypeScript Agent Runtime and multiple web application clients via WebSockets. The system leverages the actix actor framework for concurrency, state isolation, and efficient message passing—ideal for WebSocket routing and connection management.

## Core Architecture

The system is structured as a Rust workspace containing three primary crates:

```
agent-bridge-platform/
├── Cargo.toml                # Workspace definition
├── common/                   # Shared code between services
├── websocket-server/         # Core WebSocket message broker
└── web-server/               # Web API and client proxy + frontend server
```

### System Components

#### 1. WebSocket Server (Service 1)
The central hub for WebSocket connections and message routing, managing both agent and client connections.

**Core Responsibilities:**
- Accepts WebSocket connections from agents (authenticated via pre-shared key)
- Manages client session WebSocket connections from the Web Server
- Routes messages between agents and clients using actor-based concurrency
- Tracks connection state and lifecycle events

**Key Actors:**
- `AgentActor`: Handles agent runtime connections, validates authentication
- `ClientSessionActor`: Manages individual client WebSocket sessions
- `RouterActor`: Routes messages based on client ID and broadcast scenarios
- `StateManagerActor`: Maintains system-wide state and connection monitoring

#### 2. Web Server (Service 2)
Serves as the complete frontend server for the Sploots web application, handling both static assets and WebSocket proxy functionality.

**Core Responsibilities:**
- Serves the Sploots React application static assets (HTML, CSS, JS)
- Provides endpoints for client registration and connection
- Creates unique client IDs for anonymous sessions
- Proxies WebSocket connections to the WebSocket Server
- Will handle authentication in future phases
- Supports single-page application routing for the React frontend

**Key Components:**
- Static asset serving with proper SPA handling
- `ProxyActor`: Manages WebSocket connection proxying to websocket-server
- RESTful API endpoints for client registration

#### 3. Common Crate
Provides shared code, models, and utilities used by both services.

**Key Elements:**
- Message data structures (`ClientMessage`, `AgentMessage`, `SystemMessage`)
- Configuration management
- Tracing and logging utilities

### Data Flow & Communication

The platform employs a structured message flow pattern:

1. **Client Access**: Web clients access the Sploots application through the Web Server, which serves the complete React application with its 3D interface, chat components, and terminal UI.

2. **Client Connection**: When a client uses the terminal component (XTerminal), the Web Server assigns a UUID and proxies their WebSocket connection to the WebSocket Server.

3. **Agent Connection**: The Agent Runtime connects directly to the WebSocket Server, authenticated via a pre-shared token.

4. **Message Routing**:
   - Client messages are wrapped in `ClientMessage` structs and routed to the appropriate agent
   - Agent messages are wrapped in `AgentMessage` structs and routed to specific clients or broadcast
   - System messages (`SystemMessage` enum) handle connection/disconnection events

5. **Connection Management**:
   - Heartbeats maintain connection status (5-second intervals)
   - Timeout detection (30 seconds without response)
   - Exponential backoff for reconnection attempts
   - State tracking via `StateManagerActor`

## Development Roadmap

### Phase 1: Core Infrastructure ✓
- [x] Set up Cargo workspace with three crates
- [x] Implement message models in common crate
- [x] Configure basic tracing and environment variable handling
- [x] Create minimal web-server and websocket-server services

### Phase 2: Connection Management ⟳
- [x] Implement WebSocket connection handling in both services
- [x] Create `AgentActor` with pre-shared key authentication
- [x] Build `ClientSessionActor` for client session management
- [x] Add `ProxyActor` in web-server for proxying connections
- [x] Implement connection lifecycle management (register/unregister)
- [x] Add heartbeat mechanism (5s intervals, 30s timeout)
- [x] Implement reconnection with exponential backoff

### Phase 3: Web Application Integration & Authentication
- [x]        **In Progress**
- [x] Enhance web-server to serve the Sploots React application
  - [x] Configure static asset serving with proper MIME types
  - [x] Set up SPA routing for client-side routing support
  - [x] Implement compression for static assets 
- [x] Implement anonymous client ID generation
- [ ] Add SIWE (Sign-In With Ethereum) flow endpoints:
  - [ ] `/auth/challenge`: Generate SIWE message with nonce
  - [ ] `/auth/verify`: Verify signature and issue JWT
- [ ] Implement JWT validation middleware
- [ ] Create session upgrade mechanism to associate wallet addresses
- [ ] Update state management to track authenticated sessions
- [ ] Integrate WebSocket connection with the Sploots XTerminal component

### Phase 4: Message Routing Enhancement
- [ ] Expand `RouterActor` for more sophisticated routing patterns
- [ ] Implement backpressure mechanisms (mailbox limits, message prioritization)
- [ ] Add error handling with recovery strategies
- [ ] Create structured logging for message flow tracing
- [ ] Add support for structured message types from the Sploots application

### Phase 5: State Management
- [ ] Enhance `StateManagerActor` with persistence hooks
- [ ] Add metrics collection for connections and message throughput
- [ ] Implement state snapshots for recovery
- [ ] Create state change notifications via `SystemMessage`
- [ ] Implement session persistence for client reconnection

### Phase 6: Deployment and Scaling
- [ ] Finalize configuration management with the `config` crate
- [ ] Create deployment documentation for AWS EC2
- [ ] Implement multi-instance scaling with load balancing
- [ ] Set up monitoring with distributed tracing
- [ ] Configure production-ready HTTPS with Let's Encrypt

## Technical Implementation Details

### Connection Lifecycle

1. **Client Connection Process**:
   ```
   Web Client → Web Server Frontend → ProxyActor → WebSocket Server → ClientSessionActor → Router Registration
   ```

2. **Agent Connection Process**:
   ```
   Agent Runtime → WebSocket Server → AgentActor → Router Registration
   ```
   

3. **Reconnection Strategy**:
   - Detect connection failures via heartbeat timeouts
   - Attempt reconnection with exponential backoff (1s, 2s, 4s... capped at 60s)
   - Connection state tracking via `ConnectionState` enum

### Static Asset Serving

1. **Asset Organization**:
   - Serve static files from a configured directory
   - Support for client-side routing (React's BrowserRouter)
   - Handle asset cache control headers appropriately

2. **SPA Configuration**:
   - Route non-asset requests to the index.html file
   - Preserve query parameters during routing
   - Proper MIME type handling for various assets

### Message Flow

1. **Client to Agent**:
   ```
   Client → ProxyActor → ClientSessionActor → RouterActor → AgentActor → Agent Runtime
   ```

2. **Agent to Client (Direct)**:
   ```
   Agent → AgentActor → RouterActor → ClientSessionActor → ProxyActor → Client
   ```

3. **Agent to Clients (Broadcast)**:
   ```
   Agent → AgentActor → RouterActor → [ClientSessionActor₁...ₙ] → [ProxyActor₁...ₙ] → [Client₁...ₙ]
   ```

### Error Handling & Resilience

- **Connection Failures**: Detected via heartbeat mechanism
- **Message Serialization Errors**: Caught and logged without crashing
- **Actor Failures**: Isolated via actor model, with supervisor strategies
- **State Recovery**: Connection state tracking with reconnection support
- **Asset Serving Errors**: Graceful handling with appropriate error pages

## Development Setup

### Prerequisites
- Rust 1.70+ with cargo
- Node.js 16+ and npm/yarn for testing the client application
- Development environment variables (or use defaults)

### Environment Variables
```
WEBSOCKET_SERVER_ADDR=127.0.0.1:8080  # WebSocket Server binding address
WEB_SERVER_ADDR=127.0.0.1:8081        # Web Server binding address
AGENT_TOKEN=dev_token                 # Pre-shared agent authentication token
STATIC_ASSETS_PATH=./static           # Path to Sploots static assets
```

### Building the Project
```bash
# Build all crates
cargo build

# Run the WebSocket Server
cargo run -p websocket-server

# Run the Web Server
cargo run -p web-server
```

### Testing Connections
- Web Client: Access http://<web-server-addr>/ to load the Sploots application
- Direct WebSocket: Connect to `ws://<web-server-addr>/ws/<client-id>`
- Agent: Connect to `ws://<websocket-server-addr>/ws/agent` with Authorization header

## Future Extensions

### Multi-Agent Support
- Extend `RouterActor` to support multiple agent connections
- Add agent metadata and routing rules
- Implement agent-to-agent communication

### Media Streaming
- Add binary message support optimized for media
- Implement chunked transfers for large payloads
- Add compression options

### External State Storage
- Create persistence interface for Redis/PostgreSQL
- Implement state backup and recovery
- Add distributed coordination for multi-instance deployments

### Security Enhancements
- Implement rate limiting for connections and messages
- Add IP-based throttling
- Enhance token rotation and management

## Performance Considerations

### Connection Scaling
- Current design targets <500 concurrent clients
- Actor mailbox tuning for higher throughput
- Message batching for broadcast scenarios

### Static Asset Optimization
- Asset compression (Brotli/Gzip)
- Proper cache headers for client-side caching
- CDN integration capabilities

### Backpressure
- Limit mailbox size (100 messages by default)
- Drop low-priority messages under high load
- Throttle broadcast operations

### Monitoring
- Tracing instrumentation for performance metrics
- Connection count and message throughput tracking
- Latency measurement for message delivery

## Conclusion

The Agent Bridge Platform provides a robust, actor-based WebSocket messaging system designed for scalability and resilience. By following the phased development approach, the platform will grow from its current connection management capabilities to a fully-featured, authenticated messaging system with integrated web application serving, suitable for production deployment.
