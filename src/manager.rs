use std::collections::HashMap;

use crate::activetracker;
use crate::controller::Controller;
use crate::listener_stats::StatsSerde;
use crate::runner::Runner;
use crate::{
    config::Config,
    healthcheck,
    listener_stats::ListenerStats,
    resolver,
};
use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use log::{info, warn, error};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
#[derive(Debug, PartialEq, Clone)]
pub enum Status {
    STARTING,
    STARTED,
    STOPPING,
    STOPPED,
}

lazy_static! {
    static ref STATUS: Arc<RwLock<Status>> = Arc::new(RwLock::new(Status::STOPPED));
    static ref LISTENERS: Arc<RwLock<Vec<Arc<ListenerStats>>>> =
        Arc::new(RwLock::new(Vec::new()));
    static ref LISTENERS_STATUS: Arc<RwLock<HashMap<String, Result<bool, anyhow::Error>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    static ref CONTROLLER: Arc<RwLock<Controller>> = Arc::new(RwLock::new(Controller::new()));
}

pub async fn cancel() {
    info!("attempting to cancel all tasks");
    let mut w = CONTROLLER.write().await;
    w.cancel().await;
}

pub async fn get_stats(name: &str) -> Option<Arc<ListenerStats>> {
    let r = LISTENERS.read().await;
    for i in r.iter() {
        if i.name == name {
            return Some(Arc::clone(i));
        }
    }
    return None;
}

pub async fn is_running(name: &str) -> bool {
    get_stats(name).await.is_none()
}

pub async fn get_listener_stats() -> HashMap<String, StatsSerde> {
    let mut result = HashMap::new();
    let r = LISTENERS.read().await;
    for i in r.iter() {
        result.insert(i.name.clone(), StatsSerde::from(i));
    }
    return result;
}

pub async fn stop() {
    info!("stopping manager");
    let mut status = STATUS.write().await;
    if *status == Status::STOPPED {
        info!("stopping manager: succeeded (already stopped)");
        return;
    }
    let mut listeners = LISTENERS.write().await;
    let mut listener_status = LISTENERS_STATUS.write().await;
    info!("transitioning from `{status:?}` to `{:?}`", Status::STOPPING);
    *status = Status::STOPPING;
    listeners.clear();
    listener_status.clear();
    activetracker::reset().await;
    info!("cancelling all tasks");
    cancel().await;
    info!("all tasks cancelled by controller");
    *status = Status::STOPPED;
    info!("stopping manager: succeeded");
}

pub async fn get_run_status() -> Status {
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
            }
            Err(some_cause) => Err(anyhow!(format!("{some_cause}"))),
        };
        result.insert(k.clone(), v_real);
    }
    return result;
}

pub async fn start(config: Config) -> Result<HashMap<String, Result<bool>>> {
    info!("starting manager");
    let mut status = STATUS.write().await;
    if *status != Status::STOPPED {
        warn!("starting manager: failed (still running)");
        return Err(anyhow!("failed to start, still running"));
    }
    {
        let mut listeners = LISTENERS.write().await;
        let mut listener_status = LISTENERS_STATUS.write().await;
        listeners.clear();
        listener_status.clear();
    }
    // mark starting...
    *status = Status::STARTING;

    resolver::init(&config).await;
    healthcheck::init(&config).await;
    activetracker::reset().await;
    let controller_clone = Arc::clone(&CONTROLLER);
    healthcheck::start_checker(controller_clone).await;

    let config_x = Arc::new(RwLock::new(config.clone()));
    let (tx, mut rx) = mpsc::channel(config.listeners.len());
    for (name, listener) in &config.listeners {
        let name = name.clone();
        let local_config = Arc::clone(&config_x);
        let controller_local = Arc::clone(&CONTROLLER);
        let r = Runner::new(
            name.clone(),
            listener.clone(),
            local_config,
            controller_local,
        );
    
        let context = r.start();
        let mut w = CONTROLLER.write().await;
    
        let tx = tx.clone();
        w.spawn(async move {
            let result1 = context.await;
            match result1 {
                Ok(some) => {
                    LISTENERS.write().await.push(some);
                    LISTENERS_STATUS.write().await.insert(name.clone(), Ok(true));
                    info!("starting manager: {name} started OK");
                }
                Err(cause) => {
                    error!("starting manager: {name} start failed ({cause})");
                    LISTENERS_STATUS.write().await.insert(name.clone(), Err(cause));
                }
            }
            let _ = tx.send(()).await;
        }).await;
    }
    for _ in 0..config.listeners.len() {
        rx.recv().await;
    }
    info!("starting manager: succeeded");
    *status = Status::STARTED;
    //return get_listener_status();
    return Ok(get_listener_status().await);
}
