use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use log::{error, info, LevelFilter, Record};

use std::io::Write;
use std::thread;

use chrono::prelude::*;
use env_logger::fmt::Formatter;
use env_logger::Builder;
use std::error::Error;
use clap::Parser;
use std::sync::Arc;
pub mod statistics;
use statistics::{GlobalStats};
use std::time::{Duration};
use bytesize::ByteSize;

#[derive(Parser, Debug, Clone)]
pub struct CliArg {
    #[arg(short, long)]
    pub bind: Vec<String>,
    #[arg(short, long, default_value_t=30000)]
    pub ri: i32,
    #[arg(long, default_value_t=String::from(""))]
    pub log_level: String,
}

pub fn setup_logger(log_thread: bool, rust_log: Option<&str>) {
    let output_format = move |formatter: &mut Formatter, record: &Record| {
        let thread_name = if log_thread {
            format!("(t: {}) ", thread::current().name().unwrap_or("unknown"))
        } else {
            "".to_string()
        };
        let local_time: DateTime<Local> = Local::now();
        let time_str = local_time.format("%H:%M:%S%.3f").to_string();
        write!(
            formatter,
            "{} {}{} - {} - {}\n",
            time_str,
            thread_name,
            record.level(),
            record.target(),
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

async fn run_pair(laddro:String, raddro:String, g_stats:Arc<GlobalStats>) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(&laddro).await?;
    info!("Listening on: {}", &laddro);

    loop {
        // Asynchronously wait for an inbound socket.
        let (socket, _) = listener.accept().await?;
        let local_gstats = Arc::clone(&g_stats);
        let conn_stats = local_gstats.new_conn_stats();
        let conn_stats1 = Arc::clone(&conn_stats);
        let conn_stats2 = Arc::clone(&conn_stats);
        let new_active = g_stats.increase_active_conn_count();
        let raddr = raddro.clone();
        let laddr = laddro.clone();
        let conn_id = conn_stats.id();
        let conn_id = format!("Connection {conn_id}:");
        tokio::spawn(async move {
            let raddr_clone = raddr.clone();
            let laddr_clone = laddr.clone();
            let remote_address = socket.peer_addr().unwrap();
            info!("{conn_id} ({new_active}) started: from {remote_address} via {laddr_clone} to {raddr_clone}");
            'must_run: {
                let r_stream_r = TcpStream::connect(raddr).await;
                if let Err(cause) = r_stream_r.as_ref() {
                    error!("{conn_id} failed to connect to {laddr_clone}, cause {cause}");
                    break 'must_run;
                }
                let r_stream = r_stream_r.unwrap();
                let (mut lr, mut lw) = tokio::io::split(socket);
                let (mut rr, mut rw) = tokio::io::split(r_stream);

                // L -> R path
                let jh_lr = tokio::spawn( async move {
                    let mut buf = vec![0; 4096];
                    loop {
                        let n = lr
                            .read(&mut buf)
                            .await
                            .expect("failed to read data from socket");
    
                        if n == 0 {
                            return;
                        }
    
                        let write_result = rw
                            .write_all(&buf[0..n])
                            .await;
                        match write_result {
                            Err(cause) => {
                                error!("failed to write data to socket: {cause}");
                                break;
                            },
                            Ok(_) => {
                                conn_stats1.add_uploaded_bytes(n);
                            }
                        }
                    }
                });

                // R -> L path
                let jh_rl = tokio::spawn(async move {
                    let mut buf = vec![0; 4096];
                    loop {
                        let n = rr
                            .read(&mut buf)
                            .await
                            .expect("failed to read data from socket");
    
                        if n == 0 {
                            return;
                        }
    
                        let write_result = lw
                            .write_all(&buf[0..n])
                            .await;
                        match write_result {
                            Err(cause) => {
                                error!("failed to write data to socket: {cause}");
                                break;
                            },
                            Ok(_) => {
                                conn_stats2.add_downloaded_bytes(n);
                            }
                        }
                    }

                });
                jh_lr.await.unwrap();
                jh_rl.await.unwrap();
            }
            let elapsed = conn_stats.elapsed();
            let downloaded_final_v = conn_stats.downloaded_bytes();
            let uploaded_final_v = conn_stats.uploaded_bytes();
            local_gstats.add_downloaded_bytes(downloaded_final_v);
            local_gstats.add_uploaded_bytes(uploaded_final_v);
            let downloaded_final = ByteSize(downloaded_final_v as u64);
            let uploaded_final = ByteSize(uploaded_final_v as u64);

            let new_active = local_gstats.decrease_active_conn_count();
            info!("{conn_id} ({new_active}) stopped: Downloaded {downloaded_final} bytes, Uploaded {uploaded_final} bytes, Elapsed {elapsed:#?}")
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArg::parse();

    let log_level = args.log_level;
    setup_logger(true, Some(&log_level));
    for i in &args.bind {
        info!("{i}");
    }
    // Allow passing an address to listen on as the first argument of this
    // program, but otherwise we'll just set up our TCP listener on
    // 127.0.0.1:8080 for connections.

    let mut futures = Vec::new();
    let global_stats = statistics::GlobalStats::new();
    let g_stats = Arc::new(global_stats);
    for next_bind in args.bind {
        let tokens = next_bind.split("::").collect::<Vec<&str>>();
        if tokens.len() != 2 {
            error!("Each bind token must be 2 tokens separated by `::`. `{next_bind}` does not meet the requirement");
            return Ok(());
        }
        let laddr = tokens.get(0).unwrap();
        let raddr = tokens.get(1).unwrap();
        let laddr = String::from(*laddr);
        let raddr = String::from(*raddr);
        let new_g_stats = Arc::clone(&g_stats);
        let jh = tokio::spawn(async move {
            let laddr_l = laddr.clone();
            let raddr_l = raddr.clone();
            let result = run_pair(laddr, raddr, new_g_stats).await;
            if let Err(cause) = result {
                error!("error forwarding {laddr_l} -> {raddr_l} caused by {cause}");
            }
        });
        futures.push(jh);
    }

    let g_stats = Arc::clone(&g_stats);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(args.ri as u64)).await;
            let active = g_stats.active_conn_count();
            let downloaded = ByteSize(g_stats.total_downloaded_bytes() as u64);
            let uploaded = ByteSize(g_stats.total_uploaded_bytes() as u64);
            let total_conn_count = g_stats.conn_count();
            info!("**  A-{active}/T-{total_conn_count}/⬆️  {uploaded}/⬇️  {downloaded} **");
        }
    });
    for next_future in futures {
        let _ = next_future.await;
    }
    return Ok(());
    // Next up we create a TCP listener which will listen for incoming
    // connections. This TCP listener is bound to the address we determined
    // above and must be associated with an event loop.
}