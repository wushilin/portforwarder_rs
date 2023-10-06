use crate::idletracker::IdleTracker;
use anyhow::Result;
use std::sync::atomic::{Ordering, AtomicU64};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::anyhow;
use log::info;
use lazy_static::lazy_static;
use tokio::{
    io::{ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock},
    task::JoinHandle,
    time::sleep,
};

use crate::{
    config::{Config, Listener},
    listener_context::ListenerContext,
    resolver,
};

lazy_static!(
    static ref COUNTER: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
);
pub struct Runner {
    pub name:String,
    pub listener: Listener,
    pub config: Arc<RwLock<Config>>,
}

fn id() -> u64 {
    COUNTER.fetch_add(1, Ordering::SeqCst) + 1
}
impl Runner {
    pub fn new(name:String, listener: Listener, config: Arc<RwLock<Config>>) -> Runner {
        Runner { name, listener, config }
    }

    pub async fn start(&self) -> Result<Arc<RwLock<ListenerContext>>> {
        let bind = self.listener.bind.clone();
        let targets = self.listener.targets.clone();
        let idle_timeout_ms = self.config.read().await.options.max_idle_time_ms;
        let context = ListenerContext::new(Self::dummy(),&self.name, idle_timeout_ms);
        let context = Arc::new(RwLock::new(context));
        let context_clone = Arc::clone(&context);
        let reason = Arc::new(RwLock::new(String::new()));
        let reason_clone = Arc::clone(&reason);
        let jh = tokio::spawn(async move {
            let result = Self::run_listener(bind, targets, context_clone).await;
            if result.is_err() {
                let err = result.unwrap_err();
                let mut reason_local = reason_clone.write().await;
                reason_local.push_str(format!("{err}").as_str()); //= format!("{err}");
            }
        });

        sleep(Duration::from_millis(1000)).await;
        let reason_str = reason.read().await;
        if jh.is_finished() {
            return Err(anyhow!("{reason_str}"));
        }
        context.write().await.handle = jh;
        return Ok(context);
    }

    async fn run_listener(
        bind: String,
        targets: HashSet<String>,
        context: Arc<RwLock<ListenerContext>>,
    ) -> Result<()> {
        let listener = TcpListener::bind(bind).await?;
        let mut targets_vec = Vec::new();
        for next in &targets {
            targets_vec.push(next.clone());
        }
        loop {
            let (socket, _) = listener.accept().await?;
            let conn_id = id();
            // TODO
            let target_vec_clone = targets_vec.clone();
            let context = Arc::clone(&context);
            tokio::spawn(async move {
                let context_local = context;
                let context_clone = Arc::clone(&context_local);
                {
                    let ctx = context_clone.read().await;
                    let new_active = ctx.increase_conn_count();
                    let new_total = ctx.total_count();
                    let addr = socket.peer_addr();
                    if addr.is_err() {
                        return;
                    }
                    let addr = addr.unwrap();
                    info!("{conn_id} new connection from {addr:?} active {new_active} total {new_total}");

                }
                let _ = Self::worker(conn_id, target_vec_clone, socket, context_local).await;
                {
                    let ctx = context_clone.read().await;
                    let new_active = ctx.decrease_conn_count();
                    let new_total = ctx.total_count();
                    info!("{conn_id} closing connection: active {new_active} total {new_total}");
                }
            });
        }
    }

    async fn worker(
        conn_id: u64,
        targets_vec: Vec<String>,
        socket: TcpStream,
        context: Arc<RwLock<ListenerContext>>,
    ) -> Result<()> {
        // TODO
        let target = targets_vec.get(0).unwrap().clone();
        let resolved = resolver::resolve(&target).await;
        let r_stream = TcpStream::connect(&resolved).await?;
        let local_addr = r_stream.local_addr()?;
        info!("{conn_id} connected to {resolved} via {local_addr:?}");
        let (lr, lw) = tokio::io::split(socket);
        let (rr, rw) = tokio::io::split(r_stream);
        let idle_tracker = Arc::new(Mutex::new(IdleTracker::new(
            context.read().await.idle_timeout_ms,
        )));
        let context_clone = Arc::clone(&context);
        let uploaded = Arc::new(AtomicU64::new(0));
        let downloaded = Arc::new(AtomicU64::new(0));
        let jh1 = Self::pipe(conn_id, lr, rw, context_clone, Arc::clone(&idle_tracker), true, Arc::clone(&uploaded));
        let context_clone = Arc::clone(&context);
        let jh2 = Self::pipe(conn_id, rr, lw, context_clone, Arc::clone(&idle_tracker), false, Arc::clone(&downloaded));
        let context_clone = Arc::clone(&context);

        let jh = Self::run_idle_tracker(conn_id, jh1, jh2, context_clone, Arc::clone(&idle_tracker));
        let _ = jh.await;
        let uploaded_total = uploaded.load(Ordering::SeqCst);
        let downloaded_total = downloaded.load(Ordering::SeqCst);
        info!("{conn_id} end uploaded {uploaded_total} downloaded {downloaded_total}");
        Ok(())
    }
    fn dummy() -> JoinHandle<()> {
        tokio::spawn(async move {})
    }

    fn run_idle_tracker(
        conn_id: u64,
        jh1: JoinHandle<()>,
        jh2: JoinHandle<()>,
        context: Arc<RwLock<ListenerContext>>,
        idletracker: Arc<Mutex<IdleTracker>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if jh1.is_finished() || jh2.is_finished() {
                    if !jh1.is_finished() {
                        info!("{conn_id} abort upload as download stopped");
                        jh1.abort();
                    }
                    if !jh2.is_finished() {
                        info!("{conn_id} abort download as upload stopped");
                        jh2.abort();
                    }
                    info!("{conn_id} pipe ended");
                    break;
                }
                {
                    let cancel_requested = context.read().await.cancel_requested.load(Ordering::SeqCst);
                    if cancel_requested {
                        info!("{conn_id} cancel requested. aborting");
                        if !jh1.is_finished() {
                            jh1.abort();
                        }
                        if !jh2.is_finished() {
                            jh2.abort();
                        }
                        break;
                    } 
                }
                if idletracker.lock().await.is_expired() {
                    info!("{conn_id} idle time out. aborting.");
                    if !jh1.is_finished() {
                        jh1.abort();
                    }
                    if !jh2.is_finished() {
                        jh2.abort();
                    }
                    break;
                }
                sleep(Duration::from_millis(500)).await;
            }
        })
    }
    fn pipe(
        conn_id:u64, 
        reader_i: ReadHalf<TcpStream>,
        writer_i: WriteHalf<TcpStream>,
        context: Arc<RwLock<ListenerContext>>,
        idletracker: Arc<Mutex<IdleTracker>>,
        is_upload: bool,
        counter:Arc<AtomicU64>
    ) -> JoinHandle<()> {
        let mut reader = reader_i;
        let mut writer = writer_i;
        tokio::spawn(async move {
            let mut buf = vec![0; 4096];

            loop {
                let nr = reader.read(&mut buf).await;
                match nr {
                    Err(_) => {
                        break;
                    }
                    _ => {}
                }

                let n = nr.unwrap();
                if n == 0 {
                    break;
                }

                let write_result = writer.write_all(&buf[0..n]).await;
                match write_result {
                    Err(_) => {
                        break;
                    }
                    Ok(_) => {
                        counter.fetch_add(n as u64, Ordering::SeqCst);
                        if is_upload {
                            context.read().await.increase_uploaded_bytes(n);
                        } else {
                            context.read().await.increase_downloaded_bytes(n);
                        }
                        idletracker.lock().await.mark();
                    }
                }
            }
        })
    }
}
