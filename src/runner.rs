use crate::activetracker;
use crate::controller::Controller;
use crate::healthcheck;
use crate::idletracker::IdleTracker;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use log::{info, warn, error};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::{
    io::{ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock},
    task::JoinHandle,
    time::sleep,
};

use crate::{
    config::{Config, Listener},
    listener_stats::ListenerStats,
    resolver,
};

lazy_static! {
    static ref COUNTER: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
}
pub struct Runner {
    pub name: String,
    pub listener: Listener,
    pub config: Arc<RwLock<Config>>,
    pub controller: Arc<RwLock<Controller>>,
}

fn id() -> u64 {
    COUNTER.fetch_add(1, Ordering::SeqCst) + 1
}
impl Runner {
    pub fn new(
        name: String,
        listener: Listener,
        config: Arc<RwLock<Config>>,
        root_context: Arc<RwLock<Controller>>,
    ) -> Runner {
        Runner {
            name,
            listener,
            config,
            controller: root_context,
        }
    }

    pub async fn start(self) -> Result<Arc<ListenerStats>> {
        let bind = self.listener.bind.clone();
        let name = self.name.clone();
        let targets = self.listener.targets.clone();
        let mut targets_new = Vec::<String>::new();
        targets_new.extend(targets.clone());
        let idle_timeout_ms = self.config.read().await.options.max_idle_time_ms;
        let stats = ListenerStats::new(&self.name, idle_timeout_ms);
        let stats = Arc::new(stats);
        let root_context_clone = Arc::clone(&self.controller);
        let controller_clone = Arc::clone(&self.controller);
        let stats_clone = Arc::clone(&stats);
        let (tx, mut rx) = mpsc::channel(1);

        let name_clone = name.clone();
        let _ = root_context_clone
            .write()
            .await
            .spawn(async move {
                let mut listener = TcpListener::bind(&bind).await;
                let max_retry = 3;
                for i in 1..max_retry + 1 {
                    if listener.is_ok() {
                        break;
                    }
                    warn!("Listener: `{}` unable to bind to `{}` yet. retrying({i} of {max_retry})", &name_clone, &bind);
                    sleep(Duration::from_millis(100)).await;
                    listener = TcpListener::bind(&bind).await;
                }
                match listener {
                    Ok(inner_listener) => {
                        let _ = tx.send(None).await; // tell listener started successfully
                        let name_clone_result = name_clone.clone();
                        let result = Self::run_listener(
                            name_clone,
                            inner_listener,
                            targets_new,
                            stats_clone,
                            controller_clone,
                        ).await;
                        match result {
                            Err(cause) => {
                                error!("listener {name_clone_result} failed with {cause}");
                            },
                            _ => {
                                // it was ok
                            }
                        }
                    }
                    Err(cause) => {
                        let _ = tx.send(Some(format!("{cause}"))).await; // tell listener stopped successfully
                    }
                }
            })
            .await;
        let fail_reason = tokio::select! {
            _ = sleep(Duration::from_secs(1)) => {
                None
            },
            what = rx.recv() => {
                what
            }
        };
        let name = name.clone();
        match fail_reason {
            None => {
                info!("listener {name} start cancelled");
                return Ok(stats);
            }
            Some(fail_reason) => match fail_reason {
                None => {
                    warn!("listener {name} started without error");
                    return Ok(stats);
                }
                Some(cause) => {
                    info!("listener {name} start with error {cause}");
                    return Err(anyhow!("{}", cause));
                }
            },
        }
    }

    async fn run_listener(
        name: String,
        listener: TcpListener,
        targets: Vec<String>,
        stats: Arc<ListenerStats>,
        controller: Arc<RwLock<Controller>>,
    ) -> Result<()> {
        let targets_vec = Arc::new(targets);
        let name = Arc::new(name);
        loop {
            let (socket, _) = listener.accept().await?;
            let conn_id = id();
            let target_vec_clone = Arc::clone(&targets_vec);
            let stats = Arc::clone(&stats);
            let controller_clone = Arc::clone(&controller);
            let mut controller_inner = controller_clone.write().await;
            let controller_clone_inner = Arc::clone(&controller);
            let name = Arc::clone(&name);
            controller_inner.spawn(async move {
                let stats_local = Arc::clone(&stats);
                
                let addr = socket.peer_addr();
                if addr.is_err() {
                    warn!("{conn_id} connection has no peer addr");
                    return;
                }
                let addr = addr.unwrap();
                let new_active = stats_local.increase_conn_count();
                let new_total = stats_local.total_count();
                activetracker::put(conn_id, addr).await;
                info!("{conn_id} new connection from {addr:?} active {new_active} total {new_total}");
                
                let stats_local_clone = Arc::clone(&stats_local);
                let rr = Self::worker(name, conn_id, target_vec_clone, socket, stats_local_clone, controller_clone_inner).await;
                if rr.is_err() {
                    let err = rr.err().unwrap();
                    warn!("{conn_id} connection error: {err}");
                }
                let new_active = stats_local.decrease_conn_count();
                let new_total = stats_local.total_count();
                activetracker::remove(conn_id).await;
                info!("{conn_id} closing connection: active {new_active} total {new_total}");
            }).await;
        }
    }

    async fn worker(
        name: Arc<String>,
        conn_id: u64,
        targets_all: Arc<Vec<String>>,
        socket: TcpStream,
        context: Arc<ListenerStats>,
        controller: Arc<RwLock<Controller>>,
    ) -> Result<()> {
        // TODO
        // Selecting from targets_vec, consulting health check

        //let target = targets_vec.get(0).unwrap().clone();
        let (ok, target) = healthcheck::select(&name, &targets_all).await;
        if !ok {
            info!("{conn_id} selected {target} to connect (failed one)");
        } else {
            info!("{conn_id} selected {target} to connect");
        }
        let resolved = resolver::resolve(&target).await;
        info!("{conn_id} resolved {target} to {resolved}");
        let connect_future = TcpStream::connect(&resolved);
        let r_stream = tokio::time::timeout(Duration::from_secs(5), connect_future).await??;
        let local_addr = r_stream.local_addr()?;
        info!("{conn_id} connected to {resolved} via {local_addr:?}");
        let (lr, lw) = tokio::io::split(socket);
        let (rr, rw) = tokio::io::split(r_stream);
        let idle_tracker = Arc::new(Mutex::new(IdleTracker::new(context.idle_timeout_ms)));
        let context_clone = Arc::clone(&context);
        let uploaded = Arc::new(AtomicU64::new(0));
        let downloaded = Arc::new(AtomicU64::new(0));
        let controller_clone = Arc::clone(&controller);
        let jh1 = Self::pipe(
            conn_id,
            lr,
            rw,
            context_clone,
            Arc::clone(&idle_tracker),
            true,
            Arc::clone(&uploaded),
            controller_clone,
        )
        .await;
        let context_clone = Arc::clone(&context);
        let controller_clone = Arc::clone(&controller);
        let jh2 = Self::pipe(
            conn_id,
            rr,
            lw,
            context_clone,
            Arc::clone(&idle_tracker),
            false,
            Arc::clone(&downloaded),
            controller_clone,
        )
        .await;

        let controller_clone = Arc::clone(&controller);
        let jh = Self::run_idle_tracker(
            conn_id,
            jh1,
            jh2,
            Arc::clone(&idle_tracker),
            controller_clone,
        )
        .await;
        let _ = jh.await;
        let uploaded_total = uploaded.load(Ordering::SeqCst);
        let downloaded_total = downloaded.load(Ordering::SeqCst);
        info!("{conn_id} end uploaded {uploaded_total} downloaded {downloaded_total}");
        Ok(())
    }
    async fn run_idle_tracker(
        conn_id: u64,
        jh1: JoinHandle<Option<()>>,
        jh2: JoinHandle<Option<()>>,
        idletracker: Arc<Mutex<IdleTracker>>,
        root_context: Arc<RwLock<Controller>>,
    ) -> JoinHandle<Option<()>> {
        root_context
            .write()
            .await
            .spawn(async move {
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
                        break;
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
            .await
    }
    async fn pipe(
        in_conn_id: u64,
        reader_i: ReadHalf<TcpStream>,
        writer_i: WriteHalf<TcpStream>,
        context: Arc<ListenerStats>,
        idletracker: Arc<Mutex<IdleTracker>>,
        is_upload: bool,
        counter: Arc<AtomicU64>,
        controller: Arc<RwLock<Controller>>,
    ) -> JoinHandle<Option<()>> {
        let mut reader = reader_i;
        let mut writer = writer_i;
        let direction = match is_upload {
            true => "upload",
            false => "download",
        };
        let conn_id = in_conn_id;
        controller
            .write()
            .await
            .spawn(async move {
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
                                context.increase_uploaded_bytes(n);
                            } else {
                                context.increase_downloaded_bytes(n);
                            }
                            idletracker.lock().await.mark();
                        }
                    }
                }
                info!("{conn_id} {direction} ended");
            })
            .await
    }
}
