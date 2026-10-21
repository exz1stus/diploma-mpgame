pub mod client_messages;
pub mod server_messages;

pub fn app_init() {
    dotenvy::dotenv().ok();
}
