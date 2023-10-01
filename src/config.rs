use serde::{Serialize, Deserialize};
use std::error::Error;
use std::fs;
use crate::backend::{HostGroupTracker, HostGroup};
use std::collections::HashSet;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Listener {
    pub name: String,
    pub bind: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Backend {
    pub name:String,
    pub hosts: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Mapping {
    pub from: String,
    pub to: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub listeners: Vec<Listener>,
    pub backends: Vec<Backend>,
    pub forwarding: Vec<Mapping>,
    pub options: Options,
}

impl Config {
    pub fn load(filename:&str) -> Result<Config, Box<dyn Error>> {
        let content = fs::read_to_string(filename)?;
    
        let config:Config = serde_yaml::from_str(&content)?;
        return Ok(config);

    }
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    pub fn lookup_backend(&self, listener_name:&str) -> Option<String> {
        for i in &self.forwarding {
            if i.from == listener_name.to_string() {
                return Some(i.to.clone());
            }
        }
        return None;
    }
    pub fn create_backend(&self) -> HostGroupTracker {
        let mut result = HostGroupTracker::new(self.options.healthcheck_timeout_ms as u64);
        for backend in &self.backends {
            let name = &backend.name;
            let targets = &backend.hosts;
            let mut host_group = HostGroup::new(name);
            let mut target_set = HashSet::<String>::new();
            for next_host in targets {
                target_set.insert(next_host.clone());
            }
            for next_host in &target_set {
                host_group.add(next_host);
            }
            result.add(host_group);
        }
        result.start_checker();
        return result;
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Options {
    pub healthcheck_timeout_ms: i64,
    pub reporting_interval_ms: i64,
    pub dns_override_file: String,
    pub log_config_file: String,
    pub max_idle_ms: i64,
}

impl Default for Options {
    fn default() -> Self {
        Self { 
            healthcheck_timeout_ms: 5000, 
            reporting_interval_ms: 30000, 
            dns_override_file: String::from("resolve.json"), 
            log_config_file: String::from("log4rs.yaml"), 
            max_idle_ms: 600000, 
        }
    }
}