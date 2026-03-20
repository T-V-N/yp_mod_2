use crate::quote::StockQuote;
use bincode;
use std::net::SocketAddr;
use std::net::UdpSocket;
use std::thread;
use std::time::Duration;

pub struct MetricsSender {
    socket: UdpSocket,
}

impl MetricsSender {
    pub fn new(bind_addr: &str) -> Result<Self, std::io::Error> {
        let socket = UdpSocket::bind(bind_addr)?;
        Ok(Self { socket })
    }

    pub fn send_to(
        &self,
        quotes: &Vec<StockQuote>,
        target_addr: &SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = bincode::serialize(quotes)?;
        self.socket.send_to(&encoded, target_addr)?;
        Ok(())
    }

    pub fn broadcast(
        self,
        quotes: &Vec<StockQuote>,
        target_addr: &SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_to(quotes, &target_addr)
    }
}
