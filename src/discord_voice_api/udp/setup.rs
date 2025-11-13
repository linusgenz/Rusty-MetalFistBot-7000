use anyhow::Result;
use tokio::net::UdpSocket;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use tokio::net::UdpSocket as TokioUdpSocket;

const MESSAGE_LENGTH_EXCL_HEADER: u16 = 70;
const TOTAL_PACKET_SIZE: usize = 2 + 2 + MESSAGE_LENGTH_EXCL_HEADER as usize;

fn u16_from_be_bytes(b: &[u8]) -> u16 {
    ((b[0] as u16) << 8) | (b[1] as u16)
}

pub async fn discover_ip(ssrc: u32, socket: &UdpSocket) -> Result<(String, u16)> {
    let mut packet = vec![0u8; TOTAL_PACKET_SIZE];
    packet[0..2].copy_from_slice(&1u16.to_be_bytes());
    packet[2..4].copy_from_slice(&MESSAGE_LENGTH_EXCL_HEADER.to_be_bytes());
    packet[4..8].copy_from_slice(&ssrc.to_be_bytes());

    socket.send(&packet).await?;

    let mut rbuf = [0u8; 1500];
    let n = socket.recv(&mut rbuf).await?;
    if n < TOTAL_PACKET_SIZE {
        return Err(anyhow::anyhow!(
            "Unerwartete Antwortlänge beim IP discovery"
        ));
    }

    let addr_bytes = &rbuf[8..72];
    let addr_end = addr_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(addr_bytes.len());
    let address = std::str::from_utf8(&addr_bytes[..addr_end])?.to_string();
    let port = u16_from_be_bytes(&rbuf[72..74]);
    Ok((address, port))
}

pub async fn make_udp_socket(bind_addr: &str) -> Result<TokioUdpSocket> {
    let addr: SocketAddr = bind_addr.parse()?;
    let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
    let sock = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_recv_buffer_size(512 * 1024)?; // 512 KB
    sock.set_send_buffer_size(512 * 1024)?;
    sock.set_nonblocking(true)?;
    sock.bind(&addr.into())?;
    let std_sock = std::net::UdpSocket::from(sock);
    let s = TokioUdpSocket::from_std(std_sock)?;
    Ok(s)
}