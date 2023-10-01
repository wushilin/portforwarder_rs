pub mod errors;
pub mod statistics;
pub mod idletracker;
pub mod resolve;
pub mod backend;
pub mod config;
use log::{error, info, debug};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use bytesize::ByteSize;
use clap::Parser;
use statistics::ConnStats;
use std::error::Error;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use futures::lock::Mutex;

#[derive(Parser, Debug, Clone)]
pub struct CliArg {
    #[arg(short, long, default_value_t=String::from("config.yaml"), help="Location of your `config.yaml`")]
    pub config_yaml: String
}

#[derive(Debug)]
struct ExecutionContext {
    pub stats: Arc<statistics::GlobalStats>,
    pub resolver:resolve::ResolveConfig,
    pub stop: Arc<RwLock<bool>>,
    pub config: Arc<config::Config>,
    pub backend: Arc<backend::HostGroupTracker>,
}

static LOG_TGT:&str = "portforwarder";

pub fn setup_logger(log_conf_file:&str)->Result<(), Box<dyn Error>> {
    log4rs::init_file(log_conf_file, Default::default())?;
    println!("logs will be sent according to config file {log_conf_file}");
    Ok(())
}

async fn handle_socket_inner(
    socket: TcpStream,
    raddr: String,
    conn_stats: Arc<ConnStats>,
    ctx: Arc<ExecutionContext>
) -> Result<(), Box<dyn Error>> {
    let conn_id = conn_stats.id_str();
    let mut raddr_copy = raddr.clone();
    let resolved_raddr = ctx.resolver.resolve(&raddr_copy);
    if resolved_raddr.is_some() {
        info!(target:LOG_TGT, "{conn_id} resolver resolved {raddr} -> {raddr_copy}");
        raddr_copy = resolved_raddr.unwrap().clone();
    } else {
        info!(target:LOG_TGT, "{conn_id} resolver did not resolve");
    }
    info!(target:LOG_TGT, "{conn_id} connecting to {raddr_copy}...");
    let r_stream = TcpStream::connect(raddr_copy).await?;
    let local_addr = r_stream.local_addr()?;
    info!(target:LOG_TGT, "{conn_id} connected via {local_addr}");
    let (mut lr, mut lw) = tokio::io::split(socket);
    let (mut rr, mut rw) = tokio::io::split(r_stream);

    // write the header
    let conn_stats1 = Arc::clone(&conn_stats);
    let conn_stats2 = Arc::clone(&conn_stats);
    let idle_tracker = Arc::new(
        Mutex::new (
            idletracker::IdleTracker::new(ctx.config.options.max_idle_ms)
        )
    );

    let idle_tracker1 = Arc::clone(&idle_tracker);
    let idle_tracker2 = Arc::clone(&idle_tracker);
    // L -> R path
    let conn_id_local = conn_id.clone();
    let jh_lr = tokio::spawn(async move {
        let direction = ">>>";
        info!(target:LOG_TGT, "{conn_id_local} {direction} started...");
        let mut buf = vec![0; 4096];

        let conn_id = conn_stats1.id_str();
        loop {
            let nr = lr.read(&mut buf).await;
            match nr {
                Err(cause) => {
                    error!(target:LOG_TGT, "{conn_id} {direction} failed to read data from socket: {cause}");
                    return;
                }
                _ => {}
            }

            let n = nr.unwrap();
            if n == 0 {
                return;
            }

            let write_result = rw.write_all(&buf[0..n]).await;
            match write_result {
                Err(cause) => {
                    error!(target:LOG_TGT, "{conn_id} {direction} failed to write data to socket: {cause}");
                    break;
                }
                Ok(_) => {
                    conn_stats1.add_uploaded_bytes(n);
                    idle_tracker1.lock().await.mark();
                }
            }
        }
    });

    // R -> L path
    let conn_id_local = conn_id.clone();
    let jh_rl = tokio::spawn(async move {
        let direction = "<<<";
        info!(target:LOG_TGT, "{conn_id_local} {direction} started...");
        let conn_id = conn_stats2.id_str();
        let mut buf = vec![0; 4096];
        loop {
            let nr = rr.read(&mut buf).await;

            match nr {
                Err(cause) => {
                    error!(target:LOG_TGT, "{conn_id} {direction} failed to read data from socket: {cause}");
                    return;
                }
                _ => {}
            }
            let n = nr.unwrap();
            if n == 0 {
                return;
            }

            let write_result = lw.write_all(&buf[0..n]).await;
            match write_result {
                Err(cause) => {
                    error!(target:LOG_TGT, "{conn_id} {direction} failed to write data to socket: {cause}");
                    break;
                }
                Ok(_) => {
                    conn_stats2.add_downloaded_bytes(n);
                    idle_tracker2.lock().await.mark();
                }
            }
        }
    });    
    let idlechecker = tokio::spawn(
        async move {
            loop {
                if jh_lr.is_finished() && jh_rl.is_finished() {
                    debug!("{conn_id} both direction terminated gracefully");
                    break;
                }
                if *ctx.stop.read().unwrap() {
                    info!("{conn_id} stoped by context.");
                    if !jh_lr.is_finished() {
                        jh_lr.abort();
                    }
                    if !jh_rl.is_finished() {
                        jh_rl.abort();
                    }
                    break;
                }
                if idle_tracker.lock().await.is_expired() {
                    let idle_max = idle_tracker.lock().await.max_idle();
                    let idled_for = idle_tracker.lock().await.idled_for();
                    info!(target:LOG_TGT, "{conn_id} connection idled {idled_for:#?} > {idle_max:#?}. cancelling");
                    if !jh_lr.is_finished() {
                        jh_lr.abort();
                    }
                    if !jh_rl.is_finished() {
                        jh_rl.abort();
                    }
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    );
    //jh_lr.await?;
    //jh_rl.await?;
    idlechecker.await?;
    return Ok(());
}

async fn run_pair(
    listener: config::Listener,
    ctx: Arc<ExecutionContext>,
) -> Result<(), Box<dyn Error>> {
    let bind = &listener.bind;
    let name = &listener.name;
    let listener = TcpListener::bind(bind).await?;
    info!(target:LOG_TGT, "Listening on: {}", &bind);

    loop {
        if *ctx.stop.read().unwrap() {
            info!(target:LOG_TGT, "Listener `{bind}` stopped");
            return Ok(());
        }
        // Asynchronously wait for an inbound socket.
        let cstat = Arc::new(ConnStats::new(Arc::clone(&ctx.stats)));
        let conn_id = cstat.id_str();
        info!("{conn_id} begin");
        let (socket, _) = listener.accept().await?;
        let addr = socket.peer_addr();
        if addr.is_err() {
            info!(target:LOG_TGT, "{conn_id} no peer address info. Skipped.");
            continue;
        }
        let addr = addr.unwrap();
        info!(target:LOG_TGT, "{conn_id} accepted from {addr}");
        let local_gstats = Arc::clone(&ctx);
        let laddr = bind.clone();
        let backend_name = ctx.config.lookup_backend(name);
        if backend_name.is_none() {
            info!(target:LOG_TGT, "{conn_id} no backend defined for listener `{name}`, not started");
            continue;
        }
        let backend_name = backend_name.unwrap();
        let raddr= ctx.backend.select(&backend_name);
        if raddr.is_none() {
            info!(target:LOG_TGT, "{conn_id} no backend available for `{name}` -> `{backend_name}`. Not started");
            continue;
        }
        let raddr = raddr.unwrap();
        info!(target:LOG_TGT, "{conn_id} load balancer selected `{name} -> {backend_name}` -> `{raddr}`");
        tokio::spawn(async move {
            //handle_incoming(socket);
            handle_socket(socket, laddr, raddr, local_gstats, cstat, conn_id).await;
        });
    }
}

async fn handle_socket(socket: TcpStream, laddr: String, raddr: String, ctx:Arc<ExecutionContext>, cstat:Arc<ConnStats>, conn_id: String) {
    let remote_addr = socket.peer_addr();

    if remote_addr.is_err() {
        error!(target:LOG_TGT, "{conn_id} has no remote peer info. closed");
        return;
    }

    let remote_addr = remote_addr.unwrap();
    info!(target:LOG_TGT, "{conn_id} started: from {remote_addr} via {laddr}");
    let cstat_clone = Arc::clone(&cstat);
    let result = handle_socket_inner(socket, raddr, cstat_clone, ctx).await;
    let up_bytes = cstat.uploaded_bytes();
    let down_bytes = cstat.downloaded_bytes();
    let up_bytes_str = ByteSize(up_bytes as u64);
    let down_bytes_str = ByteSize(down_bytes as u64);
    let elapsed = cstat.elapsed();
    match result {
        Err(cause) => {
            error!(target:LOG_TGT, "{conn_id} failed. cause: {cause}");
        }
        Ok(_) => {}
    }
    info!(target:LOG_TGT, "{conn_id} stopped: up {up_bytes_str} down {down_bytes_str} uptime {elapsed:#?}");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArg::parse();
    let config_yaml_file = args.config_yaml;
    let config_object = config::Config::load(&config_yaml_file);
    if config_object.is_err() {
        panic!("Unable to load file `{config_yaml_file}`");
    }

    let config_object = config_object.unwrap();
    let log_conf_file = &config_object.options.log_config_file;
    match setup_logger(log_conf_file) {
        Err(cause) => {
            println!("failed to setup logger using config file `{log_conf_file}` : {cause}");
            return Ok(());
        },
        _ => {}
    }

    let bindings = (&config_object.listeners).clone();
    if bindings.len() == 0 {
        error!(target:LOG_TGT, "no binding config specified. please specify in your config file");
        return Ok(());
    }
    for i in &bindings {
        let name = &i.name;
        let bind = &i.bind;
        info!(target:LOG_TGT, "Binding config: `{name}` -> `{bind}`");
    }

    let mut resolver:resolve::ResolveConfig = Default::default();
    if config_object.options.dns_override_file != "" {
        resolver = resolve::ResolveConfig::load_from_json_file(&config_object.options.dns_override_file).expect("Unable to read the resolve config file!");
    }

    let mut listener_handles = Vec::new();
    let global_stats = statistics::GlobalStats::new();
    let lb_backend = config_object.create_backend();
    let ctx = Arc::new(
        ExecutionContext {
            stats:Arc::new(global_stats),
            resolver,
            stop: Arc::new(RwLock::new(false)),
            config: Arc::new(config_object),
            backend: Arc::new(lb_backend),
        }
    );
    info!(target:LOG_TGT, "Execution context is {ctx:#?}");
    for next_bind in bindings {
        let ctx = Arc::clone(&ctx);
        let jh = tokio::spawn(async move {
            let result = run_pair(next_bind.clone(), ctx).await;
            if let Err(cause) = result {
                error!(target:LOG_TGT, "error running portforwarder for {next_bind:?} caused by {cause}");
            }
        });
        listener_handles.push(jh);
    }

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(ctx.config.options.reporting_interval_ms as u64)).await;
            let active = ctx.stats.active_conn_count();
            let downloaded = ByteSize(ctx.stats.total_downloaded_bytes() as u64);
            let uploaded = ByteSize(ctx.stats.total_uploaded_bytes() as u64);
            let total_conn_count = ctx.stats.conn_count();
            info!(target:LOG_TGT, "**  Stats: active: {active} total: {total_conn_count} up: {uploaded} down: {downloaded} **");
        }
    });

    //tokio::time::sleep(Duration::from_secs(30)).await;
    //println!("Requesting stop...");
    //*(ctx_local.stop.write().unwrap()) = true;

    //println!("Waiting for completion...");
    //while ctx_local.stats.active_conn_count() > 0 {
    //    let new_count = ctx_local.stats.active_conn_count();
    //    println!("{new_count} active");
    //    tokio::time::sleep(Duration::from_secs(1)).await;
    //}
    //println!("All stopped");
    for next_future in listener_handles {
        let _ = next_future.await;
    }
    // println!("Exiting");
    return Ok(());
}
