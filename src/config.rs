use std::collections::{HashSet, HashMap};
use std::error::Error;
use tokio::fs;
use serde::{Serialize, Deserialize};
use serde_yaml;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub listeners: HashMap<String, Listener>,
    pub options:Options,
    pub dns: HashMap<String, String>,
    pub admin_server: Option<AdminServerConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listeners: HashMap::new(),
            options: Default::default(),
            dns: HashMap::new(),
            admin_server: None
        }
    }
}
#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
pub struct AdminServerConfig {
    pub bind_address: Option<String>,
    pub bind_port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    pub tls_ca_cert: Option<String>,
    pub mutual_tls: Option<bool>,
    pub tls: Option<bool>,
    pub rocket_log_level: Option<String>,
}

impl Default for AdminServerConfig {
    fn default() -> Self {
        AdminServerConfig { 
            bind_address: Some("0.0.0.0".into()), 
            bind_port: Some(48888), 
            username: Some("admin".into()), 
            password: Some("admin".into()),
            tls: Some(false), 
            tls_cert: Some("".into()), 
            tls_key: Some("".into()), 
            tls_ca_cert: Some("".into()), 
            mutual_tls: Some(false),
            rocket_log_level: Some("normal".into()),
        }
    }
}
impl Config {
    pub async fn load_file(filename:&str) -> Result<Config, Box<dyn Error>> {
        let content = fs::read_to_string(filename).await?;
    
        let config:Config = serde_yaml::from_str(&content)?;
        return Ok(config);
    }

    pub fn load_string(content:&str) -> Result<Config, Box<dyn Error>> {
        let config:Config = serde_yaml::from_str(&content)?;
        return Ok(config);
    }

    pub fn init_logging(&self) {
        let log_conf_file = &self.options.log_config_file;
        if log_conf_file == "" {
            println!("not initing logging as no `log4rs.yaml` defined.");
        } else {
            let result = log4rs::init_file(log_conf_file, Default::default());
            match result {
                Err(cause) => {
                    println!("failed to initialize logging from `{log_conf_file}`: {cause}");
                },
                Ok(_) =>{
                    println!("initialized logging from `{log_conf_file}`");
                }
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listener {
    pub bind: String,
    pub targets: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    pub health_check_timeout_ms: u64,
    pub log_config_file: String,
    pub max_idle_time_ms: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            health_check_timeout_ms: 0,
            log_config_file: "".into(),
            max_idle_time_ms: 0
        }
    }
}