mod receiver;

use clap::Parser;
use crossbeam::channel::unbounded;
use receiver::QuotesReceiver;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;

#[derive(Parser, Debug)]
#[command(bin_name = "quote_client")]
#[command(about = "Receive market data stream via UDP connection")]
struct Args {
    #[arg(long, short('l'), default_value = "client/cfg/tickers")]
    tickers_file: String,

    #[arg(long, short, default_value = "127.0.0.1:8971")]
    server_addr: String,

    #[arg(long, default_value = "127.0.0.1:8988")]
    udp_uri: String,

    #[arg(long, short, default_value = "6667")]
    udp_port: String,

    #[arg(long, short, default_value = "1000")]
    ping_delay: u64,

    #[arg(long, short, default_value = "")]
    output_file: String,
}

/// Reads tickers from file (one per line), returns a comma-separated list for the STREAM command.
fn tickers_csv_from_file(path: &str) -> io::Result<String> {
    let contents = fs::read_to_string(path)?;
    let tickers: Vec<String> = contents
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    if tickers.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ticker file has no tickers",
        ));
    }
    Ok(tickers.join(","))
}

fn main() -> io::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let mut stream = TcpStream::connect(args.server_addr)?;
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut response = String::new();
    reader.read_line(&mut response)?;

    if response.trim() != "Quote server ready for requests" {
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid response"));
    } else {
        log::info!("Server ready for requests");
    }

    let tickers_arg = tickers_csv_from_file(&args.tickers_file)?;
    let buf = format!("STREAM udp://127.0.0.1:{} {}\n", args.udp_port, tickers_arg);
    stream.write_all(buf.as_bytes())?;
    stream.flush()?;

    let mut response = String::new();
    reader.read_line(&mut response)?;

    if response.trim() != "OK" {
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid response"));
    }

    let w: Box<dyn Write + Send>;
    if args.output_file.is_empty() {
        w = Box::new(io::stdout());
    } else {
        let file = File::create(args.output_file)?;
        w = Box::new(BufWriter::new(file));
    }

    let (tx_receive, rx_receive) = unbounded();
    let (tx_ping, rx_ping) = unbounded();
    let receiver = QuotesReceiver::new(&args.udp_uri, &args.udp_port)?;
    if let Err(e) = ctrlc::set_handler(move || {
        log::info!("Stopping client...");
        tx_receive.send(true).unwrap();
        tx_ping.send(true).unwrap();
    }) {
        log::error!("Error setting Ctrl-C handler: {}", e);
    }

    if let Ok((ping_handle, receive_handle)) = receiver.run(args.ping_delay, w, rx_ping, rx_receive)
    {
        log::info!("Client started");
        ping_handle.join().unwrap();
        receive_handle.join().unwrap();
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Error starting client",
        ));
    }

    log::info!("Client stopped");
    Ok(())
}
