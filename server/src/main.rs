mod generator;
mod sender;

use clap::Parser;
use crossbeam::channel::unbounded;

use crate::generator::QuoteGenerator;
use crate::sender::QuotesSender;
use shared::StockQuote;

use crossbeam::channel::Sender;
use env_logger;
use std::fs;

use std::io::{self, BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};
use std::thread;

#[derive(Parser, Debug)]
#[command(bin_name = "quote_server")]
#[command(about = "TCP server for streaming stock quotes")]
struct Args {
    #[arg(long, short('l'), default_value = "server/cfg/tickers")]
    ticker_list: String,

    #[arg(long, short, default_value = "8971")]
    tcp_port: u16,

    #[arg(long, short, default_value = "8988")]
    udp_port: u16,

    #[arg(long, default_value = "2000")]
    delay_ms: u64,

    #[arg(long, default_value = "5")]
    price_deviation: u8,

    #[arg(long, default_value = "1000")]
    tick_duration_ms: u64,

    #[arg(long, short, default_value = "5000")]
    ping_cooldown_ms: u64,

    #[arg(long, short, default_value = "100")]
    capacity: u64,
}

enum Command {
    Stream(SocketAddr, Vec<String>),
    Stop(SocketAddr),
    StopSendingAll,
    Ping(SocketAddr),
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    log::info!(
        "Server ready to accept connections on TCP port {}",
        args.tcp_port
    );

    if let Err(e) = run(args) {
        log::error!("Error: unexpected error {}", e);
    }
}

fn tcp_write_line(writer: &mut impl Write, line: &[u8]) -> io::Result<()> {
    writer.write_all(line)?;
    writer.flush()
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(args.ticker_list)?;
    let ticker_list: Vec<&str> = contents.lines().collect();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", args.tcp_port))?;

    let generator = QuoteGenerator::new(
        ticker_list.clone(),
        args.price_deviation,
        args.tick_duration_ms,
    );

    let (quote_sender, quote_receiver) = unbounded::<Vec<StockQuote>>();
    let (cmd_sender, cmd_receiver) = unbounded::<Command>();
    let (cmd_sender_for_generator, cmd_receiver_for_generator) = unbounded::<Command>();

    let quotes_server = QuotesSender::new(args.capacity, args.ping_cooldown_ms, args.udp_port)?;

    let generator_handle =
        generator.stream_all_quotes(args.delay_ms, quote_sender, cmd_receiver_for_generator);

    let tick_duration_ms = args.tick_duration_ms;
    let cmd = cmd_sender.clone();
    let (runner_handle, quotes_handle) =
        quotes_server.run(quote_receiver, cmd_receiver.clone(), cmd, tick_duration_ms)?;

    let cmd_ctrlc = cmd_sender.clone();
    ctrlc::set_handler(move || {
        log::info!("Stopping server...");
        cmd_ctrlc.send(Command::StopSendingAll).unwrap();
        cmd_sender_for_generator
            .send(Command::StopSendingAll)
            .unwrap();
    })?;

    thread::spawn(move || {
        listener_loop(listener, cmd_sender).unwrap();
    });

    runner_handle.join().unwrap();
    quotes_handle.join().unwrap();
    generator_handle.join().unwrap();
    Ok(())
}

fn listener_loop(
    listener: TcpListener,
    cmd_sender: Sender<Command>,
) -> Result<(), Box<dyn std::error::Error>> {
    for stream in listener.incoming() {
        let cmd = cmd_sender.clone();
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, cmd) {
                        log::error!("tcp connection handler: {e}");
                    }
                });
            }
            Err(e) => log::error!("tcp accept failed: {e}"),
        }
    }
    Ok(())
}

fn handle_connection(
    stream: TcpStream,
    quotes_sender: Sender<Command>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    tcp_write_line(&mut writer, b"Quote server ready for requests\n")?;

    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                return Ok(());
            }
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() {
                    writer.flush()?;
                    continue;
                }

                let mut parts = input.split_whitespace();
                match parts.next() {
                    Some("STREAM") => {
                        if let Some(callback_addr) = parts
                            .next()
                            .and_then(|u| u.strip_prefix("udp://"))
                            .and_then(|host_port| host_port.parse().ok())
                        {
                            if let Some(quotes_raw) = parts.next() {
                                let ticker_list: Vec<String> = quotes_raw
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();

                                if ticker_list.is_empty() {
                                    tcp_write_line(&mut writer, b"ERR empty ticker list\n")?;
                                    continue;
                                }

                                quotes_sender
                                    .send(Command::Stream(callback_addr, ticker_list))
                                    .map_err(|e| {
                                        io::Error::new(
                                            io::ErrorKind::BrokenPipe,
                                            format!("command channel closed: {e:?}"),
                                        )
                                    })?;

                                tcp_write_line(&mut writer, b"OK\n")?;
                            } else {
                                tcp_write_line(&mut writer, b"ERR no ticker list\n")?;
                            }
                        } else {
                            tcp_write_line(&mut writer, b"ERR invalid URI\n")?;
                        }
                    }

                    Some("EXIT") => {
                        tcp_write_line(&mut writer, b"BYE\n")?;
                        return Ok(());
                    }

                    _ => {
                        tcp_write_line(&mut writer, b"ERROR: unknown command\n")?;
                    }
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}
