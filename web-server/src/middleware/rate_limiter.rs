// web-server/src/middleware/rate_limiter.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::task::{Context, Poll};
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::header,
    Error, ResponseError,
    HttpResponse
};
use futures_util::future::{LocalBoxFuture, Ready, ready};
use std::fmt;

// Client creation limits
const MAX_REQUESTS_PER_MINUTE: usize = 3;
const RATE_LIMIT_WINDOW_SECONDS: u64 = 60;

// Custom error for rate limiting
#[derive(Debug)]
struct RateLimitExceeded;

impl fmt::Display for RateLimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rate limit exceeded")
    }
}

impl ResponseError for RateLimitExceeded {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::TooManyRequests()
            .append_header((header::RETRY_AFTER, "60"))
            .body("Rate limit exceeded. Please try again later.")
    }
}

// Store for rate limit data
#[derive(Debug, Clone, Default)]
pub struct RateLimiter {
    paths: Vec<String>,
    store: Arc<Mutex<HashMap<String, (Vec<Instant>, Instant)>>>,
}

impl RateLimiter {
    pub fn new(paths: Vec<String>) -> Self {
        Self { 
            paths,
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    fn is_rate_limited(&self, ip: &str) -> bool {
        let mut store = self.store.lock().unwrap();
        let now = Instant::now();
        
        let entry = store.entry(ip.to_string()).or_insert_with(|| (Vec::new(), now));
        
        if now.duration_since(entry.1) > Duration::from_secs(60) {
            entry.0.retain(|time| now.duration_since(*time) < Duration::from_secs(RATE_LIMIT_WINDOW_SECONDS));
            entry.1 = now;
        }
        
        if entry.0.len() >= MAX_REQUESTS_PER_MINUTE {
            true
        } else {
            entry.0.push(now);
            false
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RateLimiter
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RateLimiterMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;
    
    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimiterMiddleware {
            service,
            limiter: self.clone(),
        }))
    }
}

pub struct RateLimiterMiddleware<S> {
    service: S,
    limiter: RateLimiter,
}

impl<S, B> Service<ServiceRequest> for RateLimiterMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<ServiceResponse<B>, Error>>;
    
    forward_ready!(service);
    
    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Check if this path should be rate limited
        let path = req.path().to_string();
        let should_rate_limit = self.limiter.paths.iter().any(|p| path.starts_with(p));
        
        if should_rate_limit {
            // Get client IP
            let ip = req.connection_info().realip_remote_addr()
                .unwrap_or("unknown")
                .to_string();
            
            // Check if rate limited
            if self.limiter.is_rate_limited(&ip) {
                tracing::warn!("Rate limit exceeded for IP: {}", ip);
                
                // Create error future
                return Box::pin(async { 
                    Err(RateLimitExceeded.into()) 
                });
            }
        }
        
        // Continue with the regular service
        let fut = self.service.call(req);
        Box::pin(async move {
            fut.await
        })
    }
}