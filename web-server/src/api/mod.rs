// web-server/src/api/mod.rs
pub mod sessions;

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(
        actix_web::web::scope("/api")
            .service(sessions::api_index)
            .service(sessions::create_client)
            .service(sessions::get_client_info)
            .service(sessions::invalidate_session)
    );
}