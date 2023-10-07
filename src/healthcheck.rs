use std::collections::{HashSet, HashMap};
use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use tokio::net::TcpStream;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;
use std::error::Error;
use crate::{config::Config, resolver};
use log::{info, warn};
lazy_static! {
    static ref STATUS: Arc<RwLock<HashMap<String, (bool, DateTime<Local>)>>> = Arc::new(RwLock::new(HashMap::new()));
    static ref HOSTS: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));
    static ref CHECKER_STARTED:Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
}

pub async fn init(config:&Config) {
    info!("initializing config");
    let mut hosts = HashSet::<String>::new();
    for (name, listener) in &config.listeners {
        let targets = &listener.targets;
        for target in targets {
            info!("register host `{target}` under `{name}`");
            hosts.insert(target.clone());
        }
    }
    init_inner(hosts).await;
    info!("initialization completed");
}
async fn init_inner(hosts:HashSet<String>) {
    {
        info!("clearing all host status");
        let mut statusw = STATUS.write().await;
        statusw.clear();
        info!("host status cleared");
    }
    let mut w = HOSTS.write().await;
    info!("clearing host registry");
    w.clear();
    for next in &hosts {
        info!("registering host {next}");
        w.insert(next.clone());
    }

}

pub async fn start_checker() {
    {
        let mut w = CHECKER_STARTED.write().await;
        if *w {
            // already started
            return;
        }
        *w = true;
    }
    tokio::spawn(async move {
        loop {
            // Do something
            let hosts = HOSTS.read().await;
            let mut hosts_list = Vec::new();
            let mut hosts_results = Vec::new();
            let mut check_result = Vec::<(String, bool)>::new();
            
            for i in hosts.iter() {
                hosts_list.push(i.clone());    
                hosts_results.push(check(i.clone(), Duration::from_secs(5)));
            }
            drop(hosts);

            let mut index:usize = 0;
            for fut in hosts_results {
                let result = fut.await;
                if result.is_err() {
                    let host = hosts_list.get(index).unwrap();
                    check_result.push((host.clone(), false))
                } else {
                    let result = result.unwrap();
                    check_result.push(result);
                }

                index += 1;
            }

            // Update the check_result into STATUS
            let now = Local::now();
            {
                let mut w = STATUS.write().await;
                for (host, result) in check_result {
                    if w.get(&host).is_none() {
                        // no data
                        info!("update host `{host}` to be `{result}` at {:?}", now.to_rfc3339());
                        w.insert(host, (result, now));
                    } else {
                        let (current_status, current_ts) = w.get(&host).unwrap();
                        if *current_status != result {
                            info!("update host `{host}` to be `{result}` at {:?} (was at {:?})", now.to_rfc3339(), current_ts.to_rfc3339());
                            w.insert(host, (result, now));
                        }
                    }
                }
                // w is dropped here
            }
            tokio::time::sleep(Duration::from_millis(5000)).await;
        }
    });

}

async fn check(host:String, timeout:Duration) -> Result<(String, bool), Box<dyn Error>> {
    let resolved = resolver::resolve(&host).await;
    let connect_future = TcpStream::connect(&resolved);
    let _ = tokio::time::timeout(timeout, connect_future).await??;
    return Ok((host.clone(), true));
}

pub async fn get_all_status() -> HashMap<String, (bool, DateTime<Local>)> {
    let r = STATUS.read().await;
    let mut result = HashMap::new();
    for (key, value) in r.iter() {
        result.insert(key.clone(), value.clone());
    }
    return result;
}

pub async fn get_status_for(host:&str) -> Option<(bool, DateTime<Local>)> {
    let w = STATUS.read().await;
    let result = w.get(host);
    if result.is_none() {
        return Some((true, Local::now()));
    }

    let (result, when) = result.unwrap();
    return Some((*result, when.clone()));
}

pub async fn select<'a>(name:&str, what:&'a Vec<String>) -> (bool, &'a str){
    let r = STATUS.read().await;
    let mut candidate:Vec<&String> = Vec::new();
    for host in what {
        let status = r.get(host);
        match status {
            Some(inner) => {
                if inner.0 {
                    candidate.push(&host);
                }
            },
            None =>{}
        }
    }
    if candidate.len() == 0 {
        // nothing available
        warn!("listener {name} has no available backend. randomly selecting...");
        let rand = rand::random::<usize>() % what.len();
        let selection = what.get(rand).unwrap();
        return (false, selection);
    } else {
        let rand = rand::random::<usize>() % candidate.len();
        let selection = *candidate.get(rand).unwrap();
        return (true, selection);
    }
    
}