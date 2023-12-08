use std::{sync::Arc, collections::HashMap, net::SocketAddr};

use lazy_static::lazy_static;
use tokio::sync::RwLock;

lazy_static! {
    static ref ACTIVE: Arc<RwLock<HashMap<u64, SocketAddr>>> = Arc::new(RwLock::new(HashMap::new()));
}

pub async fn reset() {
    ACTIVE.write().await.clear()
}

pub async fn put(id:u64, addr:SocketAddr) {
    let mut w = ACTIVE.write().await;
    w.insert(id, addr);
}

pub async fn remove(id:u64) {
    let mut w = ACTIVE.write().await;
    w.remove(&id);
}

pub async fn get_active_list() -> Vec<(u64, SocketAddr)> {
    let mut result = Vec::new();
    let r = ACTIVE.read().await;
    for (id, addr) in r.iter() {
        result.push((*id, *addr));
    }
    return result;
}