use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::net::IpAddr;
use thymus_common::{NetworkEvent, Protocol};
use uuid::Uuid;

/// A single parsed packet's flow-relevant metadata (no payload content).
pub struct FlowPacket {
    pub src_ip: IpAddr,
    pub src_port: u16,
    pub dst_ip: IpAddr,
    pub dst_port: u16,
    pub protocol: Protocol,
    pub length: u64,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct FlowKey {
    client_ip: IpAddr,
    server_ip: IpAddr,
    server_port: u16,
    protocol: Protocol,
}

struct FlowAccumulator {
    to_server_bytes: u64,
    to_client_bytes: u64,
    last_seen: DateTime<Utc>,
}

pub struct FlowAggregator {
    flows: HashMap<FlowKey, FlowAccumulator>,
    sensor_id: String,
    idle_timeout_secs: i64,
    max_flows: usize,
}

impl FlowAggregator {
    pub fn new(sensor_id: String, idle_timeout_secs: i64, max_flows: usize) -> Self {
        Self {
            flows: HashMap::new(),
            sensor_id,
            idle_timeout_secs,
            max_flows,
        }
    }

    /// Classify a packet into a flow key + whether the packet travels to the server.
    /// Heuristic: the server is the endpoint with the lower port (well-known service
    /// ports are low, ephemeral client ports are high). Ties break on IP ordering.
    fn classify(p: &FlowPacket) -> (FlowKey, bool) {
        use std::cmp::Ordering;

        // src is the server when it has the strictly-lower port, or on equal ports
        // when it holds the lower IP (deterministic tie-break).
        let src_is_server = match p.dst_port.cmp(&p.src_port) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => p.src_ip <= p.dst_ip,
        };

        let (client_ip, server_ip, server_port, to_server) = if src_is_server {
            // packet goes server(src) → client(dst)
            (p.dst_ip, p.src_ip, p.src_port, false)
        } else {
            // packet goes client(src) → server(dst)
            (p.src_ip, p.dst_ip, p.dst_port, true)
        };

        (
            FlowKey {
                client_ip,
                server_ip,
                server_port,
                protocol: p.protocol,
            },
            to_server,
        )
    }

    pub fn record(&mut self, packet: &FlowPacket) {
        let now = Utc::now();
        let (key, to_server) = Self::classify(packet);

        let acc = self.flows.entry(key).or_insert_with(|| FlowAccumulator {
            to_server_bytes: 0,
            to_client_bytes: 0,
            last_seen: now,
        });

        if to_server {
            acc.to_server_bytes += packet.length;
        } else {
            acc.to_client_bytes += packet.length;
        }
        acc.last_seen = now;
    }

    /// Emit flows that have been idle longer than the timeout, or force-emit the
    /// oldest flows if the table has grown past `max_flows`.
    pub fn drain_idle(&mut self) -> Vec<NetworkEvent> {
        let now = Utc::now();
        let mut emitted = Vec::new();

        let idle_keys: Vec<FlowKey> = self
            .flows
            .iter()
            .filter(|(_, acc)| (now - acc.last_seen).num_seconds() >= self.idle_timeout_secs)
            .map(|(k, _)| k.clone())
            .collect();

        for key in idle_keys {
            if let Some(acc) = self.flows.remove(&key) {
                emitted.push(self.to_event(&key, &acc));
            }
        }

        // Backpressure: if still too many flows, emit the oldest.
        if self.flows.len() > self.max_flows {
            let mut by_age: Vec<(FlowKey, DateTime<Utc>)> = self
                .flows
                .iter()
                .map(|(k, acc)| (k.clone(), acc.last_seen))
                .collect();
            by_age.sort_by_key(|(_, ts)| *ts);

            let overflow = self.flows.len() - self.max_flows;
            for (key, _) in by_age.into_iter().take(overflow) {
                if let Some(acc) = self.flows.remove(&key) {
                    emitted.push(self.to_event(&key, &acc));
                }
            }
        }

        emitted
    }

    /// Emit all remaining flows (e.g. on shutdown).
    pub fn flush(&mut self) -> Vec<NetworkEvent> {
        let drained: Vec<(FlowKey, FlowAccumulator)> = self.flows.drain().collect();
        drained
            .into_iter()
            .map(|(key, acc)| self.to_event(&key, &acc))
            .collect()
    }

    pub fn active_flows(&self) -> usize {
        self.flows.len()
    }

    fn to_event(&self, key: &FlowKey, acc: &FlowAccumulator) -> NetworkEvent {
        NetworkEvent {
            id: Uuid::new_v4(),
            timestamp: acc.last_seen,
            sensor_id: self.sensor_id.clone(),
            source_ip: key.client_ip,
            source_port: 0,
            dest_ip: key.server_ip,
            dest_port: key.server_port,
            protocol: key.protocol,
            bytes_sent: acc.to_server_bytes,
            bytes_recv: acc.to_client_bytes,
            process_pid: 0,
            process_name: String::new(),
            process_user: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    fn pkt(sip: IpAddr, sport: u16, dip: IpAddr, dport: u16, len: u64) -> FlowPacket {
        FlowPacket {
            src_ip: sip,
            src_port: sport,
            dst_ip: dip,
            dst_port: dport,
            protocol: Protocol::Tcp,
            length: len,
        }
    }

    #[test]
    fn client_to_server_direction() {
        // client (high ephemeral port) -> server:443
        let client = ip(192, 168, 1, 50);
        let server = ip(192, 168, 1, 10);
        let mut agg = FlowAggregator::new("span".into(), 0, 1000);

        agg.record(&pkt(client, 54321, server, 443, 1500));
        let events = agg.drain_idle();

        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.source_ip, client, "client is the high-port side");
        assert_eq!(e.dest_ip, server, "server is the low-port side");
        assert_eq!(e.dest_port, 443);
        assert_eq!(e.bytes_sent, 1500, "client→server bytes counted as sent");
        assert_eq!(e.bytes_recv, 0);
    }

    #[test]
    fn server_to_client_packet_normalizes_same_flow() {
        // The reverse-direction packet (server:443 -> client) must merge into the
        // same flow and count toward bytes_recv.
        let client = ip(192, 168, 1, 50);
        let server = ip(192, 168, 1, 10);
        let mut agg = FlowAggregator::new("span".into(), 0, 1000);

        agg.record(&pkt(client, 54321, server, 443, 1500)); // to server
        agg.record(&pkt(server, 443, client, 54321, 4000)); // to client
        let events = agg.drain_idle();

        assert_eq!(events.len(), 1, "both directions are one flow");
        let e = &events[0];
        assert_eq!(e.bytes_sent, 1500);
        assert_eq!(e.bytes_recv, 4000);
    }

    #[test]
    fn multiple_connections_to_same_service_aggregate() {
        let client = ip(192, 168, 1, 50);
        let server = ip(192, 168, 1, 10);
        let mut agg = FlowAggregator::new("span".into(), 0, 1000);

        // two separate client connections (different ephemeral ports) to :443
        agg.record(&pkt(client, 50001, server, 443, 1000));
        agg.record(&pkt(client, 50002, server, 443, 2000));
        let events = agg.drain_idle();

        assert_eq!(events.len(), 1, "aggregated by (client, server, port)");
        assert_eq!(events[0].bytes_sent, 3000);
    }

    #[test]
    fn idle_timeout_retains_recent_flows() {
        let mut agg = FlowAggregator::new("span".into(), 3600, 1000);
        agg.record(&pkt(ip(10, 0, 0, 1), 50000, ip(10, 0, 0, 2), 22, 500));

        // idle timeout is 1h; nothing should drain yet
        assert!(agg.drain_idle().is_empty());
        assert_eq!(agg.active_flows(), 1);

        // flush emits regardless of idle
        assert_eq!(agg.flush().len(), 1);
        assert_eq!(agg.active_flows(), 0);
    }
}
