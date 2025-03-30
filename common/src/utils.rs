// Common Crate - utils.rs 
// my-actix-system/common/src/utils.rs
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

/// Setup tracing for consistent logging across services
pub fn setup_tracing() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}