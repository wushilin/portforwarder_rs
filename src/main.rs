pub mod config;
pub mod listener_stats;
pub mod resolver;
pub mod manager;
pub mod runner;
pub mod idletracker;
pub mod adminserver;
pub mod controller;
pub mod tlsheader;
extern crate rocket;
use std::error::Error;
use config::Config;
use log::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::load_file("config.yaml").await.unwrap();
    config.init_logging();
    adminserver::init(&config).await;
    let start_result = manager::start(config).await;
    match start_result {
        Ok(result) => {
            for (name, inner_result) in result {
                match inner_result {
                    Ok(inner_start_result) => {
                        if inner_start_result {
                            info!("started listener {name}");
                        } else {
                            info!("started listener {name} (false)");
                        }
                    },
                    Err(inner_start_err) => {
                        info!("start listner {name} error: {inner_start_err}");
                    }
                }
            }
        },
        Err(cause) => {
            error!("failed to start all listeners: {cause}");
        }
    }
    let _ = adminserver::run_rocket().await?;
    Ok(())
}
