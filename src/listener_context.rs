use std::{time::Duration, sync::{Arc, atomic::{AtomicUsize, Ordering, AtomicBool}}};
use serde::{Serialize, Deserialize};
use tokio::{task::JoinHandle, time::sleep};
use log::info;
pub struct ListenerContext {
    pub name:String,
    pub cancel_requested: Arc<AtomicBool>,
    pub handle:JoinHandle<()>,
    pub idle_timeout_ms: u64,
    pub total: Arc<AtomicUsize>,
    pub active: Arc<AtomicUsize>,
    pub downloaded_bytes: Arc<AtomicUsize>,
    pub uploaded_bytes: Arc<AtomicUsize>
}

#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
pub struct Stats {
    name: String,
    total: usize,
    active: usize,
    downloaded_bytes: usize,
    uploaded_bytes: usize,
}

impl Stats {
    pub fn from(context:&ListenerContext) -> Self {
        Self {
            name: context.name.clone(),
            total: context.total_count(),
            active: context.active_count(),
            downloaded_bytes: context.downloaded_bytes_count(),
            uploaded_bytes: context.uploaded_bytes_count(),
        }
    }
}

impl ListenerContext {
    pub async fn cancel(&self) {
        let name = &self.name;
        info!("cancelling context for `{name}`");
        self.cancel_requested.store(true, Ordering::SeqCst);
        loop {
            let active = self.active_count();
            if active == 0 {
                break;
            }
            sleep(Duration::from_millis(500)).await;
        }
        info!("`{}` has no more connection", name);

        if self.handle.is_finished() {
            info!("`{name}` gracefully ended");
            return;
        }
        self.handle.abort();
        loop {
            if self.handle.is_finished() {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        info!("`{name}` ended on request.");
    }

    fn newau() -> Arc<AtomicUsize> {
        return Arc::new(AtomicUsize::new(0));
    }
    pub fn new(handle:JoinHandle<()>, name:&str, idletimeout:u64) -> Self {
        Self {
            name: name.into(),
            cancel_requested: Arc::new(AtomicBool::new(false)),
            handle,
            idle_timeout_ms: idletimeout,
            total: Self::newau(),
            active: Self::newau(),
            downloaded_bytes: Self::newau(),
            uploaded_bytes: Self::newau(),
        }
    }
    pub fn increase_conn_count(&self) -> usize {
        self.total.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn decrease_conn_count(&self) -> usize {
        self.active.fetch_sub(1, Ordering::SeqCst) - 1
    }

    pub fn total_count(&self) -> usize {
        self.total.load(Ordering::SeqCst)
    }

    pub fn active_count(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    pub fn increase_uploaded_bytes(&self, count:usize) -> usize {
        self.uploaded_bytes.fetch_add(count, Ordering::SeqCst) + count
    }

    pub fn increase_downloaded_bytes(&self, count:usize) -> usize {
        self.downloaded_bytes.fetch_add(count, Ordering::SeqCst) + count
    }

    pub fn uploaded_bytes_count(&self) -> usize {
        self.uploaded_bytes.load(Ordering::SeqCst)
    }

    pub fn downloaded_bytes_count(&self) -> usize {
        self.downloaded_bytes.load(Ordering::SeqCst)
    }
}