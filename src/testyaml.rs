pub mod backend;
pub mod config;
pub mod resolve;
pub mod errors;

use tokio::{fs::File, io::AsyncReadExt};
use config::Config;
use std::error::Error;
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let file = "config.yaml";
    let mut file = File::open(file).await?;
    let mut dest = String::new();
    let _x = file.read_to_string(&mut dest).await;
    println!("{dest}");

    let config: Config = serde_yaml::from_str(&dest)?;

    println!("{config:?}");
    Ok(())
}