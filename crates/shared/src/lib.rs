pub mod messages;

pub fn app_init() {
    dotenvy::dotenv().ok();
}
