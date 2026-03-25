use crossbeam::channel::{Receiver, TryRecvError};
use shared::StockQuote;
use std::io::{ErrorKind, Write};
use std::net::UdpSocket;
use std::thread;
use std::time::Duration;

pub struct QuotesReceiver {
    socket: UdpSocket,
}

impl QuotesReceiver {
    pub fn new(server_udp_addr: &str, bind_port: &str) -> Result<Self, std::io::Error> {
        let socket = UdpSocket::bind(format!("127.0.0.1:{}", bind_port))?;
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        socket.connect(server_udp_addr)?;
        log::info!("Receiver ignited at {}", bind_port);
        Ok(Self { socket })
    }

    fn receive_loop<W: Write>(
        self,
        cmd_chan: Receiver<bool>,
        mut out: W,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; 1024];

        log::info!("Waiting for data...");

        loop {
            match cmd_chan.try_recv() {
                Ok(false) | Err(TryRecvError::Empty) => {}
                Ok(true) | Err(TryRecvError::Disconnected) => {
                    log::info!("quotes receiver: stopping udp receiver");
                    return Ok(());
                }
            }

            match self.socket.recv_from(&mut buf) {
                Ok((size, _)) => match bincode::deserialize::<StockQuote>(&buf[..size]) {
                    Ok(quote) => {
                        if let Err(e) = out.write_fmt(format_args!("{:}\n", quote)) {
                            log::error!("Error writing to out: {}", e);
                        }
                        if let Err(e) = out.flush() {
                            log::error!("Error flushing: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Error deserialization: {}", e);
                    }
                },
                Err(e) => match e.kind() {
                    ErrorKind::WouldBlock | ErrorKind::TimedOut => continue,
                    _ => {
                        log::error!("Error receiving data: {}", e);
                        return Err(Box::new(e));
                    }
                },
            }
        }
    }

    pub fn run<W: Write + Send + 'static>(
        self,
        ping_delay: u64,
        out: W,
        rx_ping: Receiver<bool>,
        rx_receive: Receiver<bool>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ping_socket = self.socket.try_clone()?;

        let ping_handle = thread::spawn(move || {
            if let Err(e) = ping_loop(ping_socket, ping_delay, rx_ping) {
                log::error!("ping loop error: {}", e);
            }
        });

        if let Err(e) = self.receive_loop(rx_receive, out) {
            log::error!("receiver loop error: {}", e);
        }

        ping_handle.join().expect("Error joining ping thread");

        Ok(())
    }
}

fn ping_loop(
    socket: UdpSocket,
    ping_delay: u64,
    cmd_chan: Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match cmd_chan.try_recv() {
            Ok(false) | Err(TryRecvError::Empty) => {}

            Ok(true) | Err(TryRecvError::Disconnected) => {
                log::info!("quotes receiver: stopping ping");
                return Ok(());
            }
        }
        if let Err(e) = socket.send(b"PING") {
            log::error!("quotes receiver: ping error");
            return Err(Box::new(e));
        }

        thread::sleep(Duration::from_millis(ping_delay));
    }
}
