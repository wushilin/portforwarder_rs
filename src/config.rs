use std::collections::HashMap;
use std::error::Error;
use regex::Regex;
use tokio::fs;
use serde_yaml;
use serde_derive::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub listeners: HashMap<String, Listener>,
    pub options:Options,
    pub dns: HashMap<String, String>,
    pub admin_server: Option<AdminServerConfig>,
}

impl Listener {
    pub fn max_idle_time_ms(&self) -> u64 {
        match self.max_idle_time_ms.as_ref() {
            Some(inner) => {
                if *inner == 0 {
                    return u64::MAX;
                } else {
                    return *inner;
                }
            },
            None => {
                return 3600000;
            }
        }
    }

    fn match_host(&self, host:&str) -> bool {
        for static_host in &self.rules.static_hosts {
            if host.to_ascii_lowercase() == static_host.to_ascii_lowercase() {
                return true;
            }
        }

        for next_regex in &self.rules.patterns {
            if next_regex.is_match(host) {
                return true;
            }
        }
        return false;
    }
    pub fn is_allowed(&self, host:&str) -> bool {
        let matched = self.match_host(host);
        match self.policy {
            Policy::ALLOW => {
                matched
            }, 
            Policy::DENY => {
                !matched
            }
        }
    }
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
    pub target_port: u16,
    pub policy: Policy,
    pub rules: Rules,
    pub max_idle_time_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rules {
    pub static_hosts: Vec<String>,
    #[serde(with = "serde_regex")]
    pub patterns: Vec<Regex>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Policy {
    ALLOW,
    DENY
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    pub log_config_file: String,
    pub self_ips: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            log_config_file: "".into(),
            self_ips: Vec::new()
        }
    }
}