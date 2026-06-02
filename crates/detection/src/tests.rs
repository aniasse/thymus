#[cfg(test)]
mod tests {
    use chrono::Utc;
    use std::net::{IpAddr, Ipv4Addr};
    use thymos_common::*;
    use uuid::Uuid;

    use crate::ImmuneEngine;

    fn make_profile() -> MachineIdentity {
        let mut profile = MachineIdentity::new("test-machine".into(), "test-machine".into());
        profile.temporal.active_hour_start = 8;
        profile.temporal.active_hour_end = 18;
        profile.temporal.avg_daily_volume = 300_000_000; // 300 MB

        profile.relational.known_peers.push(PeerProfile {
            peer_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            peer_hostname: Some("db-rh".into()),
            ports: vec![5432],
            protocols: vec![Protocol::Tcp],
            direction: ConnectionDirection::Outgoing,
            avg_daily_volume: 100_000_000,
            avg_daily_connections: 50.0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            confidence: 0.9,
        });

        profile.relational.known_peers.push(PeerProfile {
            peer_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            peer_hostname: Some("mail".into()),
            ports: vec![25, 587],
            protocols: vec![Protocol::Tcp],
            direction: ConnectionDirection::Outgoing,
            avg_daily_volume: 50_000_000,
            avg_daily_connections: 20.0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            confidence: 0.9,
        });

        profile
    }

    fn make_event(dest_ip: Ipv4Addr, dest_port: u16, bytes_sent: u64) -> NetworkEvent {
        NetworkEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now()
                .date_naive()
                .and_hms_opt(10, 0, 0)
                .unwrap()
                .and_utc(),
            sensor_id: "test-machine".into(),
            source_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
            source_port: 54321,
            dest_ip: IpAddr::V4(dest_ip),
            dest_port,
            protocol: Protocol::Tcp,
            bytes_sent,
            bytes_recv: 0,
            process_pid: 1234,
            process_name: "app".into(),
            process_user: "user".into(),
        }
    }

    #[test]
    fn normal_traffic_no_mutation() {
        let engine = ImmuneEngine::new();
        let profile = make_profile();

        // Connection to known peer on known port
        let event = make_event(Ipv4Addr::new(192, 168, 1, 10), 5432, 1000);
        let result = engine.analyze_network_event(&event, &profile);
        assert!(
            result.is_none(),
            "normal traffic should not trigger mutation"
        );
    }

    #[test]
    fn unknown_peer_triggers_mutation() {
        let engine = ImmuneEngine::new();
        let profile = make_profile();

        // Connection to unknown IP
        let event = make_event(Ipv4Addr::new(10, 0, 0, 99), 443, 1000);
        let result = engine.analyze_network_event(&event, &profile);
        assert!(result.is_some(), "unknown peer should trigger mutation");

        let mutation = result.unwrap();
        assert!(mutation.risk_score > 0.5);
        assert!(mutation.dimensions.contains(&MutationDimension::Relational));
    }

    #[test]
    fn malicious_port_triggers_innate() {
        let engine = ImmuneEngine::new();
        let profile = make_profile();

        // Connection to port 4444 (known malicious)
        let event = make_event(Ipv4Addr::new(10, 0, 0, 1), 4444, 1000);
        let result = engine.analyze_network_event(&event, &profile);
        assert!(result.is_some(), "malicious port should trigger mutation");

        let mutation = result.unwrap();
        assert!(mutation.innate_score >= 0.8);
        assert!(mutation.risk_score >= 0.8);
    }

    #[test]
    fn both_layers_amplify_score() {
        let engine = ImmuneEngine::new();
        let profile = make_profile();

        // Unknown peer + malicious port = combined amplification
        let event = make_event(Ipv4Addr::new(10, 0, 0, 1), 4444, 1000);
        let result = engine.analyze_network_event(&event, &profile);
        let mutation = result.unwrap();

        assert!(
            mutation.risk_score > mutation.innate_score,
            "combined score should exceed individual scores"
        );
        assert!(
            mutation.risk_score > mutation.adaptive_score,
            "combined score should exceed individual scores"
        );
    }

    #[test]
    fn known_peer_unknown_port_below_threshold() {
        let engine = ImmuneEngine::new();
        let profile = make_profile();

        // Known peer (db-rh) but unusual port — score is 0.4 (borderline)
        // which is at-threshold, not above it, so no mutation
        let event = make_event(Ipv4Addr::new(192, 168, 1, 10), 8080, 1000);
        let result = engine.analyze_network_event(&event, &profile);
        assert!(
            result.is_none(),
            "known peer with unusual port should be borderline, not flagged"
        );
    }

    #[test]
    fn scoring_combine_amplifies_dual_detection() {
        use crate::scoring::combine_scores;

        assert!(combine_scores(0.0, 0.0) < 0.01);
        assert!((combine_scores(0.8, 0.0) - 0.8).abs() < 0.01);
        assert!((combine_scores(0.0, 0.8) - 0.8).abs() < 0.01);

        let dual = combine_scores(0.5, 0.5);
        assert!(dual > 0.5, "dual detection should amplify: got {dual}");
    }
}
