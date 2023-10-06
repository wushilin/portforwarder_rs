use crate::idletracker::IdleTracker;
use anyhow::Result;
use std::sync::atomic::Ordering;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::anyhow;
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

pub struct Runner {
    pub name:String,
    pub listener: Listener,
    pub config: Arc<RwLock<Config>>,
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
                    println!("New connection: active {new_active} total {new_total}");

                }
                let _ = Self::worker(target_vec_clone, socket, context_local).await;
                {
                    let ctx = context_clone.read().await;
                    let new_active = ctx.decrease_conn_count();
                    let new_total = ctx.total_count();
                    println!("Closing connection: active {new_active} total {new_total}");
                }
            });
        }
    }

    async fn worker(
        targets_vec: Vec<String>,
        socket: TcpStream,
        context: Arc<RwLock<ListenerContext>>,
    ) -> Result<()> {
        let target = targets_vec.get(0).unwrap().clone();
        let resolved = resolver::resolve(&target).await;
        let r_stream = TcpStream::connect(&resolved).await?;
        let (lr, lw) = tokio::io::split(socket);
        let (rr, rw) = tokio::io::split(r_stream);
        let idle_tracker = Arc::new(Mutex::new(IdleTracker::new(
            context.read().await.idle_timeout_ms,
        )));
        let context_clone = Arc::clone(&context);
        let jh1 = Self::pipe(lr, rw, context_clone, Arc::clone(&idle_tracker), true);
        let context_clone = Arc::clone(&context);
        let jh2 = Self::pipe(rr, lw, context_clone, Arc::clone(&idle_tracker), false);
        let context_clone = Arc::clone(&context);

        let jh = Self::run_idle_tracker(jh1, jh2, context_clone, Arc::clone(&idle_tracker));
        let _ = jh.await;
        println!("JH is done");
        Ok(())
    }
    fn dummy() -> JoinHandle<()> {
        tokio::spawn(async move {})
    }

    fn run_idle_tracker(
        jh1: JoinHandle<()>,
        jh2: JoinHandle<()>,
        context: Arc<RwLock<ListenerContext>>,
        idletracker: Arc<Mutex<IdleTracker>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if jh1.is_finished() && jh2.is_finished() {
                    println!("Pipe gracefully down");
                    break;
                }
                {
                    let cancel_requested = context.read().await.cancel_requested.load(Ordering::SeqCst);
                    println!("Cancel requested: {cancel_requested}");
                    if cancel_requested {
                        println!("Aborting PIPE!!!");
                        if !jh1.is_finished() {
                            jh1.abort();
                        }
                        if !jh2.is_finished() {
                            jh2.abort();
                        }
                        break;
                    } else {
                        println!("Cancel is not requested");
                    }
                }
                if idletracker.lock().await.is_expired() {
                    println!("Aborting expired PIPE!!!");
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
        reader_i: ReadHalf<TcpStream>,
        writer_i: WriteHalf<TcpStream>,
        context: Arc<RwLock<ListenerContext>>,
        idletracker: Arc<Mutex<IdleTracker>>,
        is_upload: bool,
    ) -> JoinHandle<()> {
        let mut reader = reader_i;
        let mut writer = writer_i;
        tokio::spawn(async move {
            let mut buf = vec![0; 4096];

            loop {
                let nr = reader.read(&mut buf).await;
                match nr {
                    Err(_) => {
                        return;
                    }
                    _ => {}
                }

                let n = nr.unwrap();
                if n == 0 {
                    return;
                }

                let write_result = writer.write_all(&buf[0..n]).await;
                match write_result {
                    Err(_) => {
                        break;
                    }
                    Ok(_) => {
                        if is_upload {
                            context.write().await.increase_uploaded_bytes(n);
                        } else {
                            context.write().await.increase_downloaded_bytes(n);
                        }
                        idletracker.lock().await.mark();
                    }
                }
            }
        })
    }
}
