use log::info;

mod error;
mod storage;
mod types;

pub fn initialize_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .format_module_path(true)
        .init();

    info!("Logger initialized");
}

fn main() {
    initialize_logger();

    // Continue with the rest of your application
    info!("Application starting up");

    // ... your code here ...
    println!("Hello, world!");

    info!("Application shutting down");
}
