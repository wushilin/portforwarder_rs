use std::collections::HashMap;

use lazy_static::lazy_static;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::{config::Config, resolver, healthcheck, listener_context::{ListenerContext, Stats}};
use tokio::time::sleep;
use std::time::Duration;
use anyhow::{Result, anyhow};
use crate::runner::Runner;
use log::info;
#[derive(Debug, PartialEq, Clone)]
pub enum Status {
    STARTING,
    STARTED,
    STOPPING,
    STOPPED,
}

lazy_static!(
    static ref STATUS: Arc<RwLock<Status>> = Arc::new(RwLock::new(Status::STOPPED));
    static ref LISTENERS: Arc<RwLock<Vec<Arc<RwLock<ListenerContext>>>>> = Arc::new(RwLock::new(Vec::new()));
    static ref LISTENERS_STATUS: Arc<RwLock<HashMap<String, Result<bool, anyhow::Error>>>> = Arc::new(RwLock::new(HashMap::new()));
);

pub async fn get_stats(name:&str) -> Option<Stats> {
    let r = LISTENERS.read().await;
    for i in r.iter() {
        let ir = i.read().await;
        if ir.name == name {
            return Some(Stats::from(&ir));
        }
    }
    return None;
}

pub async fn is_running(name:&str) -> bool {
    get_stats(name).await.is_none()
}

pub async fn get_listener_stats() -> HashMap<String, Stats> {
    let mut result = HashMap::new();
    let r = LISTENERS.read().await;
    for i in r.iter() {
        let ir = i.read().await;
        result.insert(ir.name.clone(), Stats::from(&ir));
    }
    return result;
}

pub async fn stop() {
    let mut status = STATUS.write().await;
    let mut listeners = LISTENERS.write().await;
    *status = Status::STOPPING;
    info!("stopping all listeners.");
    for i in 0..listeners.len() {
        let next = listeners.get_mut(i).unwrap();
        let next = next.read().await;
        let name = next.name.clone();
        info!("stopping listener `{name}`");
        next.cancel().await;
        info!("stopped listener `{name}`");
    }
    listeners.clear();
    *status = Status::STOPPED;
    info!("all listeners stopped");
}

pub async fn get_run_status()->Status {
    let r = STATUS.read().await;
    return r.clone();
}


pub async fn get_listener_status() -> HashMap<String, Result<bool, anyhow::Error>> {
    let status_read = LISTENERS_STATUS.read().await;
    let mut result = HashMap::new();
    for (k, v) in status_read.iter() {
        let v_real = match v {
            Ok(result) => {
                if *result {
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
            Err(some_cause) => {
                Err(anyhow!(format!("{some_cause}")))
            }
        };
        result.insert(k.clone(), v_real);
    }
    return result;
}
pub async fn start(config:Config) -> Result<HashMap<String, Result<bool>>> {
    let mut status = STATUS.write().await;
    let mut listeners = LISTENERS.write().await;
    let mut listener_status = LISTENERS_STATUS.write().await;
    if *status != Status::STOPPED {
        return Err(anyhow!("failed to start, still running"));
    }
    listeners.clear();
    listener_status.clear();
    // mark starting...
    *status = Status::STARTING;

    resolver::init(&config).await;
    healthcheck::init(&config).await;
    healthcheck::start_checker().await;
    
    let config_x = Arc::new(RwLock::new(config.clone()));
    for (name, listener) in &config.listeners {
        let local_config = Arc::clone(&config_x);
        let r = Runner::new(name.clone(), listener.clone(), local_config);
        let context = r.start().await;
        sleep(Duration::from_millis(100)).await;
        match context {
            Ok(some) => {
                listeners.push(some);
                listener_status.insert(name.clone(), Ok(true));
            },
            Err(cause) => {
                listener_status.insert(name.clone(), Err(cause));
            }
        }
    }
    *status = Status::STARTED;
    drop(status);
    drop(listeners);
    drop(listener_status);
    //return get_listener_status();
    return Ok(get_listener_status().await);
}

