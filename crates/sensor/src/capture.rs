use anyhow::{Result, anyhow};
use std::net::IpAddr;
use thymus_common::Protocol;

use pnet_datalink::{Channel, Config, DataLinkReceiver};
use pnet_packet::Packet;
use pnet_packet::ethernet::{EtherTypes, EthernetPacket};
use pnet_packet::ip::IpNextHeaderProtocols;
use pnet_packet::ipv4::Ipv4Packet;
use pnet_packet::tcp::TcpPacket;
use pnet_packet::udp::UdpPacket;

use crate::flows::FlowPacket;

pub fn open(interface_name: &str) -> Result<Box<dyn DataLinkReceiver>> {
    let interfaces = pnet_datalink::interfaces();
    let iface = interfaces
        .into_iter()
        .find(|i| i.name == interface_name)
        .ok_or_else(|| anyhow!("interface '{interface_name}' not found"))?;

    match pnet_datalink::channel(&iface, Config::default()) {
        Ok(Channel::Ethernet(_tx, rx)) => Ok(rx),
        Ok(_) => Err(anyhow!("unsupported channel type")),
        Err(e) => Err(anyhow!("failed to open capture channel: {e}")),
    }
}

/// Parse an Ethernet frame into flow metadata. Returns None for non-IPv4 or
/// non-TCP/UDP traffic. Only headers are read — no payload content.
pub fn parse_frame(frame: &[u8]) -> Option<FlowPacket> {
    let eth = EthernetPacket::new(frame)?;

    if eth.get_ethertype() != EtherTypes::Ipv4 {
        return None;
    }

    let ipv4 = Ipv4Packet::new(eth.payload())?;
    let src_ip = IpAddr::V4(ipv4.get_source());
    let dst_ip = IpAddr::V4(ipv4.get_destination());
    let length = u64::from(ipv4.get_total_length());

    match ipv4.get_next_level_protocol() {
        IpNextHeaderProtocols::Tcp => {
            let tcp = TcpPacket::new(ipv4.payload())?;
            Some(FlowPacket {
                src_ip,
                src_port: tcp.get_source(),
                dst_ip,
                dst_port: tcp.get_destination(),
                protocol: Protocol::Tcp,
                length,
            })
        }
        IpNextHeaderProtocols::Udp => {
            let udp = UdpPacket::new(ipv4.payload())?;
            Some(FlowPacket {
                src_ip,
                src_port: udp.get_source(),
                dst_ip,
                dst_port: udp.get_destination(),
                protocol: Protocol::Udp,
                length,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    use pnet_packet::ethernet::MutableEthernetPacket;
    use pnet_packet::ip::IpNextHeaderProtocols;
    use pnet_packet::ipv4::MutableIpv4Packet;
    use pnet_packet::tcp::MutableTcpPacket;

    /// Build a minimal Ethernet/IPv4/TCP frame for parser testing.
    fn build_tcp_frame(src: Ipv4Addr, sport: u16, dst: Ipv4Addr, dport: u16) -> Vec<u8> {
        const ETH_HDR: usize = 14;
        const IP_HDR: usize = 20;
        const TCP_HDR: usize = 20;
        let total = ETH_HDR + IP_HDR + TCP_HDR;
        let mut buf = vec![0u8; total];

        {
            let mut eth = MutableEthernetPacket::new(&mut buf[..]).unwrap();
            eth.set_ethertype(EtherTypes::Ipv4);
        }
        {
            let mut ip = MutableIpv4Packet::new(&mut buf[ETH_HDR..]).unwrap();
            ip.set_version(4);
            ip.set_header_length(5);
            ip.set_total_length(u16::try_from(IP_HDR + TCP_HDR).unwrap());
            ip.set_next_level_protocol(IpNextHeaderProtocols::Tcp);
            ip.set_source(src);
            ip.set_destination(dst);
        }
        {
            let mut tcp = MutableTcpPacket::new(&mut buf[ETH_HDR + IP_HDR..]).unwrap();
            tcp.set_source(sport);
            tcp.set_destination(dport);
            tcp.set_data_offset(5);
        }

        buf
    }

    #[test]
    fn parses_tcp_frame() {
        let frame = build_tcp_frame(
            Ipv4Addr::new(192, 168, 1, 50),
            54321,
            Ipv4Addr::new(192, 168, 1, 10),
            443,
        );

        let fp = parse_frame(&frame).expect("should parse");
        assert_eq!(fp.src_ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)));
        assert_eq!(fp.src_port, 54321);
        assert_eq!(fp.dst_ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)));
        assert_eq!(fp.dst_port, 443);
        assert_eq!(fp.protocol, Protocol::Tcp);
        assert_eq!(fp.length, 40);
    }

    #[test]
    fn rejects_non_ipv4() {
        let mut buf = vec![0u8; 14];
        {
            let mut eth = MutableEthernetPacket::new(&mut buf[..]).unwrap();
            eth.set_ethertype(EtherTypes::Arp);
        }
        assert!(parse_frame(&buf).is_none());
    }

    #[test]
    fn rejects_truncated_frame() {
        assert!(parse_frame(&[0u8; 4]).is_none());
    }
}
