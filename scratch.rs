
# Phase 3 Implementation Plan: Web Application Integration & Authentication

## 1. Overview
Phase 3 focuses on enhancing the web-server to serve the Sploots React application and implementing a complete authentication system using Sign-In With Ethereum (SIWE). This includes static asset serving, SPA routing support, client identity management, and secure wallet-based authentication with JWT issuance.

## 2. Technical Requirements

### Static Asset Serving
- Configure the web-server to serve the Sploots React application
- Implement proper MIME type handling for various assets
- Support client-side routing (React's BrowserRouter)
- Configure compression for static assets
- Set up CORS handling for development and production environments

### Anonymous Client Management
- Implement client ID (UUID) generation for anonymous sessions
- Add client session persistence for non-authenticated users
- Ensure seamless session upgrades for initially anonymous clients

### Web3 Authentication System
- Implement SIWE challenge generation endpoint
- Develop signature verification with wallet address validation
- Create JWT issuance and validation system
- Implement session upgrade mechanism with wallet address association
- Add session persistence for authenticated users

### Secure WebSocket Integration
- Secure the WebSocket connection with authenticated sessions
- Develop proper error handling for authentication failures
- Ensure seamless connection to Sploots XTerminal component
- Support re-authentication for persistent connections

## 3. Implementation Tasks

### 3.1 Static Asset Serving
- [x] Implement StaticFiles middleware for the web-server
- [x] Configure SPA routing for client-side navigation
- [x] Add proper MIME type mappings for all Sploots assets
- [x] Implement file compression (gzip/brotli)
- [x] Add cache-control headers for optimal caching


### 3.2 Anonymous Client Management
- [x] Develop ClientRegistry service for tracking anonymous clients
- [x]  Implement secure UUID generation for client identifiers
- [x] Add session persistence mechanisms (cookies/localStorage)
- [x] Create client session API endpoints
- [x] Implement anonymous session timeout and cleanup
- [x] Add metrics collection for client sessions

### 3.3 Simplified Wallet Authentication System
- [x] Add JWT token utilities to common crate
- [x] Update ClientSession model to support wallet addresses and JWT tokens
- [x] Create session upgrade endpoint for wallet address association
- [x] Implement JWT generation with proper claims
- [ ] Add JWT validation middleware
- [ ] Create secure token storage mechanisms
- [x] Implement session upgrade for anonymous clients
- [x] Add wallet address association with client sessions

### 3.4 WebSocket Authentication Integration
- [ ] Enhance ProxyActor with authentication state
- [ ] Update ClientSessionActor to handle authenticated clients
- [ ] Implement JWT validation for WebSocket connections
- [ ] Add secure reconnection with session persistence
- [ ] Develop authentication error handling
- [ ] Implement message authentication for sensitive operations

## 4. Testing Strategy

### 4.1 Unit Tests
- [ ] Test static asset serving with various file types
- [ ] Test SPA routing for client-side paths
- [ ] Test SIWE challenge/verification flow
- [ ] Test JWT generation and validation
- [ ] Test authenticated WebSocket connections

### 4.2 Integration Tests
- [ ] Test complete authentication flow from challenge to JWT
- [ ] Test session persistence across page reloads
- [ ] Test client session upgrades from anonymous to authenticated
- [ ] Test authenticated WebSocket messaging
- [ ] Test proper error handling for authentication failures

### 4.3 Security Tests
- [ ] Test JWT validation and expiration
- [ ] Test against replay attacks in the authentication flow
- [ ] Test signature validation with various wallet types
- [ ] Test authentication timeout and token refresh
- [ ] Test against common authentication vulnerabilities

## 5. Implementation Approach

###  1: Static Asset Serving
- Implement basic static file serving in web-server
- Configure SPA routing for React application
- Set up development CORS and compression
- Create tests for asset serving
- Integrate with existing Sploots application

###  2: Anonymous Client Management
- Implement UUID generation for client identifiers
- Develop session persistence mechanisms
- Create API endpoints for client registration
- Add session tracking and metrics
- Create tests for anonymous sessions

###  3: Web3 Authentication
- Implement SIWE challenge/verification flow
- Develop JWT generation and validation
- Create authentication middleware
- Set up secure token storage
- Add session upgrade mechanisms
- Create tests for authentication flow

###  4: WebSocket Authentication & Integration
- Enhance WebSocket connections with authentication
- Update session actors to support authenticated clients
- Integrate with Sploots XTerminal component
- Add secure reconnection with tokens
- Create comprehensive tests for the full system
- Document the authentication architecture

## 6. Success Criteria
- Sploots application loads correctly from the web-server
- SPA routing works correctly for all application paths
- Anonymous clients can connect and use basic features
- Users can authenticate with their Ethereum wallets
- JWT tokens are securely generated and validated
- Authenticated sessions persist across page reloads
- WebSocket connections maintain authentication state
- Session upgrades work seamlessly for anonymous clients
- Connection security is maintained throughout the system
