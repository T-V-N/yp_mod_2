mod generator;
mod quote;
mod sender;

use clap::Parser;
use crossbeam::channel::unbounded;

use crate::generator::QuoteGenerator;
use crate::quote::StockQuote;
use crate::sender::MetricsSender;

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Write, stdin, stdout};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[command(bin_name = "quote_server")]
#[command(about = "Provide market data stream via UDP connection")]
struct Args {
    #[arg(long, short, default_value = "server/cfg/tickers.txt")]
    ticker_list: String,
    #[arg(long, short, default_value = "8971")]
    port: String,
    #[arg(long, short, default_value = "5")]
    delay_s: u8,

    #[arg(long, short, default_value = "5")]
    price_deviation: u8,

    #[arg(long, short, default_value = "1000")]
    tick_duration_ms: u64,

    #[arg(long, short, default_value = "5000")]
    ping_cooldown_ms: u64,
}

enum ConnectionResult {
    Exit,
    Lost,
}

fn main() {
    let args = Args::parse();
    let mut output = stdout();

    if let Err(e) = run(args, &mut output) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run<W: Write>(args: Args, w: W) -> Result<(), Box<dyn std::error::Error>> {
    let _input_file = File::open(&args.ticker_list)?;
    let contents = fs::read_to_string(args.ticker_list)?;
    let ticker_list = contents.split("\n").collect::<Vec<&str>>();
    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.port))?;

    let generator = QuoteGenerator::new(
        ticker_list.clone(),
        args.price_deviation,
        args.tick_duration_ms,
    );

    let (s, r) = unbounded::<Vec<StockQuote>>();
    let market_cache = RwLock::new(HashMap::<String, StockQuote>::new());
    thread::spawn(move || {
        generator.stream_all_quotes(args.tick_duration_ms, s);
        while let Ok(quotes) = r.recv() {
            let mut guard = market_cache.write().unwrap();
            for quote in quotes {
                guard.insert(quote.ticker.clone(), quote);
            }
        }
    });

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    handle_connection(stream, args.ping_cooldown_ms);
                });
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }
    Ok(())
}

pub fn handle_connection(stream: TcpStream, ping_cooldown_ms: u64) {
    let mut writer = stream.try_clone().expect("failed to clone stream");
    let mut reader = BufReader::new(stream);

    let _ = writer.write_all(b"Quote server ready for requests\n");
    let _ = writer.flush();

    let mut line = String::new();

    let mut last_ping = AtomicU64::new(current_time());

    loop {
        line.clear();
        // read_line ждёт '\n' — nc отправляет строку по нажатию Enter
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF — клиент закрыл соединение
                return;
            }
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() {
                    let _ = writer.flush();
                    continue;
                }

                let mut parts = input.split_whitespace();
                let response = match parts.next() {
                    Some("STREAM") => {
                        let uri = parts.next();
                        if let Some(uri) = uri.and_then(|uri| uri.parse::<SocketAddr>().ok()) {
                            let sender = MetricsSender::new(&uri.to_string()).unwrap();
                        } else {
                            continue;
                        }
                    }

                    Some("PING") => {
                        last_ping.store(current_time(), Ordering::SeqCst);
                    }

                    Some("EXIT") => {
                        let _ = writer.write_all(b"BYE\n");
                        let _ = writer.flush();
                        return;
                    }

                    _ => {
                        let _ = writer.write_all(b"ERROR: unknown command\n");
                        let _ = writer.flush();
                        return;
                    }
                };
            }
            Err(_) => {
                // ошибка чтения — закрываем
                return;
            }
        }
    }
}

fn current_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn stream_quotes(ticker_list: Vec<&str>, delay_s: u8) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
