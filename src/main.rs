pub mod errors;
pub mod statistics;
pub mod idletracker;
use log::{error, info, LevelFilter, Record, debug};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use bytesize::ByteSize;
use chrono::prelude::*;
use clap::Parser;
use env_logger::fmt::Formatter;
use env_logger::Builder;
use statistics::{ConnStats};
use std::error::Error;
use std::io::Write;
use std::sync::Arc;
use std::thread;
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
    #[arg(long, default_value_t=String::from("INFO"), help="log level argument (ERROR INFO WARN DEBUG)")]
    pub log_level: String,
    #[arg(long, default_value_t=300, help="close connection after max idle in seconds")]
    pub max_idle: i32,
}

#[derive(Debug)]
struct ExecutionContext {
    pub max_idle: i32,
    pub stats: Arc<statistics::GlobalStats>,
}

pub fn setup_logger(log_thread: bool, rust_log: Option<&str>) {
    let output_format = move |formatter: &mut Formatter, record: &Record| {
        let thread_name = if log_thread {
            format!("(t: {}) ", thread::current().name().unwrap_or("unknown"))
        } else {
            "".to_string()
        };
        let local_time: DateTime<Local> = Local::now();
        let time_str = local_time.to_rfc3339_opts(SecondsFormat::Millis, true);
        write!(
            formatter,
            "{} {}{: >5} - {}\n",
            time_str,
            thread_name,
            record.level(),
            record.args()
        )
    };

    let mut builder = Builder::new();
    builder
        .format(output_format)
        .filter(None, LevelFilter::Info);

    rust_log.map(|conf| builder.parse_filters(conf));

    builder.init();
}

async fn handle_socket_inner(
    socket: TcpStream,
    raddr: String,
    conn_stats: Arc<ConnStats>,
    ctx: Arc<ExecutionContext>
) -> Result<(), Box<dyn Error>> {
    let conn_id = conn_stats.id_str();
    info!("{conn_id} connecting to {raddr}...");
    let r_stream = TcpStream::connect(raddr).await?;
    let local_addr = r_stream.local_addr()?;
    info!("{conn_id} connected via {local_addr}");
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
                    error!("{conn_id} {direction} failed to read data from socket: {cause}");
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
                    error!("{conn_id} {direction} failed to write data to socket: {cause}");
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
                    error!("{conn_id} {direction} failed to read data from socket: {cause}");
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
                    error!("{conn_id} {direction} failed to write data to socket: {cause}");
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
                    info!("{conn_id} connection idled {idled_for:#?} > {idle_max:#?}. cancelling");
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
    info!("Listening on: {}", &bind);

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
        error!("{conn_id} has no remote peer info. closed");
        return;
    }

    let remote_addr = remote_addr.unwrap();
    info!("{conn_id} started: from {remote_addr} via {laddr}");
    let cstat_clone = Arc::clone(&cstat);
    let result = handle_socket_inner(socket, raddr, cstat_clone, ctx).await;
    let up_bytes = cstat.uploaded_bytes();
    let down_bytes = cstat.downloaded_bytes();
    let up_bytes_str = ByteSize(up_bytes as u64);
    let down_bytes_str = ByteSize(down_bytes as u64);
    let elapsed = cstat.elapsed();
    match result {
        Err(cause) => {
            error!("{conn_id} failed. cause: {cause}");
        }
        Ok(_) => {}
    }
    info!("{conn_id} stopped: up {up_bytes_str} down {down_bytes_str} uptime {elapsed:#?}");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArg::parse();

    let log_level = args.log_level;
    let max_idle = args.max_idle;
    setup_logger(false, Some(&log_level));

    if args.bind.len() == 0 {
        error!("no binding config specified. please specify using `-b` or `--bind`");
        return Ok(());
    }
    for i in &args.bind {
        info!("Binding config: {i}");
    }

    let mut futures = Vec::new();
    let global_stats = statistics::GlobalStats::new();
    let ctx = Arc::new(
        ExecutionContext {
            max_idle,
            stats:Arc::new(global_stats),
        }
    );
    info!("Execution context is {ctx:#?}");
    for next_bind in args.bind {
        let tokens = next_bind.split("::").collect::<Vec<&str>>();
        if tokens.len() != 2 {
            error!("invalid specification {next_bind}");
            continue;
        }
        let bind_addr = String::from(tokens[0]);
        let forward_addr = String::from(tokens[1]);
        let bind_c = next_bind.clone();
        let ctx = Arc::clone(&ctx);
        let jh = tokio::spawn(async move {
            let result = run_pair(bind_addr, forward_addr, ctx).await;
            if let Err(cause) = result {
                error!("error running tlsproxy for {bind_c} caused by {cause}");
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
            info!("**  Stats: active: {active} total: {total_conn_count} up: {uploaded} down: {downloaded} **");
        }
    });

    for next_future in futures {
        let _ = next_future.await;
    }

    return Ok(());
}
