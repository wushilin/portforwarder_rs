pub mod errors;
pub mod statistics;
pub mod idletracker;
use log::{error, info, debug};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use bytesize::ByteSize;
use clap::Parser;
use statistics::{ConnStats};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use futures::lock::{Mutex};

#[derive(Parser, Debug, Clone)]
pub struct CliArg {
    #[arg(
        short,
        long,
        help = "forward config `bind_ip:bind_port::forward_host:forward_port` format (repeat for multiple)"
    )]
    pub bind: Vec<String>,
    #[arg(
        short,
        long,
        default_value_t = 30000,
        help = "stats report interval in ms"
    )]
    pub ri: i32,
    #[arg(long, default_value_t=300, help="close connection after max idle in seconds")]
    pub max_idle: i32,
    #[arg(long, default_value_t=String::from("log4rs.yaml"), help="log4rs config yaml file path")]
    pub log_conf_file:String
}

#[derive(Debug)]
struct ExecutionContext {
    pub max_idle: i32,
    pub stats: Arc<statistics::GlobalStats>,
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
    info!(target:LOG_TGT, "{conn_id} connecting to {raddr}...");
    let r_stream = TcpStream::connect(raddr).await?;
    let local_addr = r_stream.local_addr()?;
    info!(target:LOG_TGT, "{conn_id} connected via {local_addr}");
    let (mut lr, mut lw) = tokio::io::split(socket);
    let (mut rr, mut rw) = tokio::io::split(r_stream);

    // write the header
    let conn_stats1 = Arc::clone(&conn_stats);
    let conn_stats2 = Arc::clone(&conn_stats);
    let idle_tracker = Arc::new(
        Mutex::new (
            idletracker::IdleTracker::new(Duration::from_secs(ctx.max_idle as u64))
        )
    );

    let idle_tracker1 = Arc::clone(&idle_tracker);
    let idle_tracker2 = Arc::clone(&idle_tracker);
    // L -> R path
    let jh_lr = tokio::spawn(async move {
        let direction = ">>>";
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
    let jh_rl = tokio::spawn(async move {
        let direction = "<<<";
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
    bind: String,
    forward: String,
    ctx: Arc<ExecutionContext>,
) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(&bind).await?;
    info!(target:LOG_TGT, "Listening on: {}", &bind);

    loop {
        // Asynchronously wait for an inbound socket.
        let (socket, _) = listener.accept().await?;
        let local_gstats = Arc::clone(&ctx);
        let laddr = bind.clone();
        let raddr = forward.clone();
        tokio::spawn(async move {
            //handle_incoming(socket);
            let _ = handle_socket(socket, laddr, raddr, local_gstats).await;
        });
    }
}

async fn handle_socket(socket: TcpStream, laddr: String, raddr: String, ctx:Arc<ExecutionContext>) {
    let cstat = Arc::new(ConnStats::new(Arc::clone(&ctx.stats)));
    let conn_id = cstat.id_str();
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

    let max_idle = args.max_idle;
    let log_conf_file = args.log_conf_file;
    match setup_logger(&log_conf_file) {
        Err(cause) => {
            println!("failed to setup logger using config file `{log_conf_file}` : {cause}");
            return Ok(());
        },
        _ => {}
    }

    if args.bind.len() == 0 {
        error!(target:LOG_TGT, "no binding config specified. please specify using `-b` or `--bind`");
        return Ok(());
    }
    for i in &args.bind {
        info!(target:LOG_TGT, "Binding config: {i}");
    }

    let mut futures = Vec::new();
    let global_stats = statistics::GlobalStats::new();
    let ctx = Arc::new(
        ExecutionContext {
            max_idle,
            stats:Arc::new(global_stats),
        }
    );
    info!(target:LOG_TGT, "Execution context is {ctx:#?}");
    for next_bind in args.bind {
        let tokens = next_bind.split("::").collect::<Vec<&str>>();
        if tokens.len() != 2 {
            error!(target:LOG_TGT, "invalid specification {next_bind}");
            continue;
        }
        let bind_addr = String::from(tokens[0]);
        let forward_addr = String::from(tokens[1]);
        let bind_c = next_bind.clone();
        let ctx = Arc::clone(&ctx);
        let jh = tokio::spawn(async move {
            let result = run_pair(bind_addr, forward_addr, ctx).await;
            if let Err(cause) = result {
                error!(target:LOG_TGT, "error running tlsproxy for {bind_c} caused by {cause}");
            }
        });
        futures.push(jh);
    }

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(args.ri as u64)).await;
            let active = ctx.stats.active_conn_count();
            let downloaded = ByteSize(ctx.stats.total_downloaded_bytes() as u64);
            let uploaded = ByteSize(ctx.stats.total_uploaded_bytes() as u64);
            let total_conn_count = ctx.stats.conn_count();
            info!(target:LOG_TGT, "**  Stats: active: {active} total: {total_conn_count} up: {uploaded} down: {downloaded} **");
        }
    });

    for next_future in futures {
        let _ = next_future.await;
    }

    return Ok(());
}
