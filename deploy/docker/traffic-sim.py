#!/usr/bin/env python3
"""Simulates normal network traffic by sending fake events to the core."""

import json
import os
import random
import time
import uuid
from datetime import datetime, timezone

import urllib.request

CORE_URL = os.environ.get("CORE_URL", "http://core:9443")
SENSOR_ID = os.environ.get("SENSOR_ID", "rh-01")
INTERVAL = int(os.environ.get("INTERVAL", "5"))

NORMAL_PEERS = {
    "rh-01": [
        ("192.168.1.10", 5432, "db-rh"),
        ("192.168.1.20", 25, "mail"),
        ("192.168.1.20", 587, "mail"),
        ("192.168.1.50", 443, "proxy"),
    ],
    "finance-01": [
        ("192.168.2.10", 5432, "db-finance"),
        ("192.168.2.20", 8080, "erp"),
        ("192.168.1.50", 443, "proxy"),
    ],
    "it-01": [
        ("192.168.3.10", 22, "backup"),
        ("192.168.3.20", 53, "dns"),
        ("192.168.1.50", 443, "proxy"),
        ("192.168.1.10", 5432, "db-rh"),
        ("192.168.2.10", 5432, "db-finance"),
    ],
}

def make_event(sensor_id, dest_ip, dest_port):
    return {
        "id": str(uuid.uuid4()),
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "sensor_id": sensor_id,
        "source_ip": f"192.168.{random.randint(1,3)}.{random.randint(100,200)}",
        "source_port": random.randint(30000, 60000),
        "dest_ip": dest_ip,
        "dest_port": dest_port,
        "protocol": "Tcp",
        "bytes_sent": random.randint(100, 50000),
        "bytes_recv": random.randint(100, 50000),
        "process_pid": random.randint(1000, 9000),
        "process_name": random.choice(["python3", "postgres", "nginx", "ssh", "curl"]),
        "process_user": "appuser",
    }

def send_batch(events):
    batch = {
        "sensor_id": SENSOR_ID,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "network_events": events,
        "process_events": [],
        "system_events": [],
    }
    data = json.dumps(batch).encode()
    req = urllib.request.Request(
        f"{CORE_URL}/api/events",
        data=data,
        headers={"Content-Type": "application/json"},
    )
    try:
        urllib.request.urlopen(req, timeout=5)
        print(f"[{SENSOR_ID}] sent {len(events)} events")
    except Exception as e:
        print(f"[{SENSOR_ID}] error: {e}")

def main():
    peers = NORMAL_PEERS.get(SENSOR_ID, NORMAL_PEERS["rh-01"])
    print(f"[{SENSOR_ID}] starting traffic sim → {CORE_URL}")
    print(f"[{SENSOR_ID}] peers: {len(peers)}")

    while True:
        events = []
        for dest_ip, dest_port, _ in peers:
            if random.random() < 0.7:
                events.append(make_event(SENSOR_ID, dest_ip, dest_port))

        if events:
            send_batch(events)
        time.sleep(INTERVAL)

if __name__ == "__main__":
    main()
