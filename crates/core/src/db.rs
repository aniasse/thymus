use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use thymos_common::{
    ConnectionDirection, MachineIdentity, MemoryCell, Mutation, MutationStatus, PeerProfile,
    ResponseAction, ToleranceEntry,
};

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("thymos.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS machines (
                machine_id TEXT PRIMARY KEY,
                hostname TEXT NOT NULL,
                first_seen TEXT NOT NULL,
                profile_maturity REAL DEFAULT 0.0,
                observation_days INTEGER DEFAULT 0,
                active_hour_start INTEGER DEFAULT 0,
                active_hour_end INTEGER DEFAULT 23,
                avg_daily_volume INTEGER DEFAULT 0,
                avg_daily_connections REAL DEFAULT 0.0,
                hourly_volumes TEXT DEFAULT '',
                active_days TEXT DEFAULT '',
                last_updated TEXT NOT NULL,
                discovery TEXT DEFAULT 'Agent'
            );
            CREATE TABLE IF NOT EXISTS peers (
                machine_id TEXT NOT NULL,
                peer_ip TEXT NOT NULL,
                peer_hostname TEXT,
                ports TEXT NOT NULL,
                protocols TEXT NOT NULL,
                direction TEXT NOT NULL,
                avg_daily_volume INTEGER DEFAULT 0,
                avg_daily_connections REAL DEFAULT 0.0,
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL,
                confidence REAL DEFAULT 0.1,
                PRIMARY KEY (machine_id, peer_ip)
            );
            CREATE TABLE IF NOT EXISTS mutations (
                id TEXT PRIMARY KEY,
                detected_at TEXT NOT NULL,
                machine_id TEXT NOT NULL,
                risk_score REAL NOT NULL,
                innate_score REAL NOT NULL,
                adaptive_score REAL NOT NULL,
                dimensions TEXT NOT NULL,
                status TEXT NOT NULL,
                response TEXT,
                details TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memory_cells (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tolerances (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn save_phase(&self, phase: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('phase', ?1)",
            [phase],
        )?;
        Ok(())
    }

    pub fn load_phase(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = 'phase'")?;
        Ok(stmt.query_row([], |row| row.get::<_, String>(0)).ok())
    }

    pub fn save_event_count(&self, count: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('event_count', ?1)",
            [count.to_string()],
        )?;
        Ok(())
    }

    pub fn load_event_count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = 'event_count'")?;
        Ok(stmt
            .query_row([], |row| row.get::<_, String>(0))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    pub fn save_profiles(&self, profiles: &HashMap<String, MachineIdentity>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        tx.execute("DELETE FROM machines", [])?;
        tx.execute("DELETE FROM peers", [])?;

        for profile in profiles.values() {
            let hourly_json = serde_json::to_string(&profile.temporal.avg_hourly_volume)?;
            let days_json = serde_json::to_string(&profile.temporal.active_days)?;

            tx.execute(
                "INSERT INTO machines VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                rusqlite::params![
                    profile.machine_id,
                    profile.hostname,
                    profile.first_seen.to_rfc3339(),
                    profile.profile_maturity,
                    profile.observation_days,
                    profile.temporal.active_hour_start,
                    profile.temporal.active_hour_end,
                    profile.temporal.avg_daily_volume,
                    profile.temporal.avg_daily_connections,
                    hourly_json,
                    days_json,
                    profile.last_updated.to_rfc3339(),
                    format!("{:?}", profile.discovery),
                ],
            )?;

            for peer in &profile.relational.known_peers {
                tx.execute(
                    "INSERT INTO peers VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    rusqlite::params![
                        profile.machine_id,
                        peer.peer_ip.to_string(),
                        peer.peer_hostname,
                        serde_json::to_string(&peer.ports)?,
                        serde_json::to_string(&peer.protocols)?,
                        format!("{:?}", peer.direction),
                        peer.avg_daily_volume,
                        peer.avg_daily_connections,
                        peer.first_seen.to_rfc3339(),
                        peer.last_seen.to_rfc3339(),
                        peer.confidence,
                    ],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub fn load_profiles(&self) -> Result<HashMap<String, MachineIdentity>> {
        let conn = self.conn.lock().unwrap();
        let mut profiles = HashMap::new();

        let mut stmt = conn.prepare(
            "SELECT machine_id, hostname, first_seen, profile_maturity, observation_days,
             active_hour_start, active_hour_end, avg_daily_volume, avg_daily_connections,
             hourly_volumes, active_days, last_updated, discovery FROM machines",
        )?;

        let machine_rows: Vec<_> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u8>(5)?,
                    row.get::<_, u8>(6)?,
                    row.get::<_, u64>(7)?,
                    row.get::<_, f64>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                ))
            })?
            .flatten()
            .collect();

        for row in machine_rows {
            let machine_id = row.0.clone();
            let mut profile = MachineIdentity::new(machine_id.clone(), row.1);
            if let Ok(ts) = row.2.parse() {
                profile.first_seen = ts;
            }
            profile.profile_maturity = row.3;
            profile.observation_days = row.4;
            profile.temporal.active_hour_start = row.5;
            profile.temporal.active_hour_end = row.6;
            profile.temporal.avg_daily_volume = row.7;
            profile.temporal.avg_daily_connections = row.8;
            if let Ok(v) = serde_json::from_str(&row.9) {
                profile.temporal.avg_hourly_volume = v;
            }
            if let Ok(d) = serde_json::from_str(&row.10) {
                profile.temporal.active_days = d;
            }
            if let Ok(ts) = row.11.parse() {
                profile.last_updated = ts;
            }
            profile.discovery = match row.12.as_str() {
                "Passive" => thymos_common::Discovery::Passive,
                _ => thymos_common::Discovery::Agent,
            };

            let mut peer_stmt = conn.prepare("SELECT * FROM peers WHERE machine_id = ?1")?;
            let peer_rows = peer_stmt.query_map([&machine_id], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, u64>(6)?,
                    r.get::<_, f64>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, f64>(10)?,
                ))
            })?;

            for pr in peer_rows.flatten() {
                let Ok(peer_ip) = pr.0.parse() else {
                    continue;
                };
                profile.relational.known_peers.push(PeerProfile {
                    peer_ip,
                    peer_hostname: pr.1,
                    ports: serde_json::from_str(&pr.2).unwrap_or_default(),
                    protocols: serde_json::from_str(&pr.3).unwrap_or_default(),
                    direction: match pr.4.as_str() {
                        "Incoming" => ConnectionDirection::Incoming,
                        "Both" => ConnectionDirection::Both,
                        _ => ConnectionDirection::Outgoing,
                    },
                    avg_daily_volume: pr.5,
                    avg_daily_connections: pr.6,
                    first_seen: pr.7.parse().unwrap_or_else(|_| chrono::Utc::now()),
                    last_seen: pr.8.parse().unwrap_or_else(|_| chrono::Utc::now()),
                    confidence: pr.9,
                });
            }

            profiles.insert(machine_id, profile);
        }

        Ok(profiles)
    }

    pub fn save_mutations(&self, mutations: &[Mutation]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM mutations", [])?;

        for m in mutations {
            tx.execute(
                "INSERT INTO mutations VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                rusqlite::params![
                    m.id.to_string(),
                    m.detected_at.to_rfc3339(),
                    m.machine_id,
                    m.risk_score,
                    m.innate_score,
                    m.adaptive_score,
                    serde_json::to_string(&m.dimensions)?,
                    format!("{:?}", m.status),
                    m.response.as_ref().map(|r| format!("{r:?}")),
                    serde_json::to_string(&m.details)?,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn load_mutations(&self) -> Result<Vec<Mutation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, detected_at, machine_id, risk_score, innate_score,
             adaptive_score, dimensions, status, response, details FROM mutations",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?;

        let mut mutations = Vec::new();
        for row in rows.flatten() {
            mutations.push(Mutation {
                id: row.0.parse().unwrap_or_else(|_| uuid::Uuid::new_v4()),
                detected_at: row.1.parse().unwrap_or_else(|_| chrono::Utc::now()),
                machine_id: row.2,
                risk_score: row.3,
                innate_score: row.4,
                adaptive_score: row.5,
                dimensions: serde_json::from_str(&row.6).unwrap_or_default(),
                status: match row.7.as_str() {
                    "Investigating" => MutationStatus::Investigating,
                    "Resolved" => MutationStatus::Resolved,
                    "FalsePositive" => MutationStatus::FalsePositive,
                    _ => MutationStatus::Active,
                },
                response: row.8.and_then(|r| match r.as_str() {
                    "Monitor" => Some(ResponseAction::Monitor),
                    "ThrottleBandwidth" => Some(ResponseAction::ThrottleBandwidth),
                    "BlockNewConnections" => Some(ResponseAction::BlockNewConnections),
                    "Isolate" => Some(ResponseAction::Isolate),
                    _ => None,
                }),
                details: serde_json::from_str(&row.9).unwrap_or_default(),
            });
        }

        Ok(mutations)
    }

    pub fn save_memory_cells(&self, cells: &[MemoryCell]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM memory_cells", [])?;

        for cell in cells {
            let data = serde_json::to_string(cell)?;
            tx.execute(
                "INSERT INTO memory_cells (id, created_at, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![cell.id.to_string(), cell.created_at.to_rfc3339(), data],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn load_memory_cells(&self) -> Result<Vec<MemoryCell>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT data FROM memory_cells")?;
        let cells = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .flatten()
            .filter_map(|data| serde_json::from_str(&data).ok())
            .collect();
        Ok(cells)
    }

    pub fn save_tolerances(&self, tolerances: &[ToleranceEntry]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM tolerances", [])?;

        for t in tolerances {
            let data = serde_json::to_string(t)?;
            tx.execute(
                "INSERT INTO tolerances (id, data) VALUES (?1, ?2)",
                rusqlite::params![t.id.to_string(), data],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn load_tolerances(&self) -> Result<Vec<ToleranceEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT data FROM tolerances")?;
        let entries = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .flatten()
            .filter_map(|data| serde_json::from_str(&data).ok())
            .collect();
        Ok(entries)
    }
}
