#!/usr/bin/env python3
"""Simulates an attack: lateral movement from RH to Finance to exfiltration."""

import json
import os
import time
import uuid
from datetime import datetime, timezone

import urllib.request

CORE_URL = os.environ.get("CORE_URL", "http://core:9443")
SCENARIO = os.environ.get("SCENARIO", "lateral_movement")

def send_event(sensor_id, dest_ip, dest_port, bytes_sent=1000, process="app"):
    batch = {
        "sensor_id": sensor_id,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "network_events": [{
            "id": str(uuid.uuid4()),
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "sensor_id": sensor_id,
            "source_ip": "192.168.1.150",
            "source_port": 54321,
            "dest_ip": dest_ip,
            "dest_port": dest_port,
            "protocol": "Tcp",
            "bytes_sent": bytes_sent,
            "bytes_recv": 0,
            "process_pid": 9999,
            "process_name": process,
            "process_user": "root",
        }],
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
    except Exception as e:
        print(f"error: {e}")

def activate():
    req = urllib.request.Request(f"{CORE_URL}/api/activate", method="POST")
    try:
        urllib.request.urlopen(req, timeout=5)
        print("[*] immune detection activated")
    except Exception as e:
        print(f"error activating: {e}")

def wait_for_core():
    print("[*] waiting for core...")
    for _ in range(30):
        try:
            urllib.request.urlopen(f"{CORE_URL}/api/health", timeout=2)
            return
        except Exception:
            time.sleep(1)
    print("[!] core not reachable")

def lateral_movement():
    print("=" * 60)
    print("ATTACK SIMULATION: Lateral Movement")
    print("=" * 60)

    wait_for_core()

    print("\n[*] waiting 30s for profiles to build...")
    time.sleep(30)

    activate()
    time.sleep(2)

    print("\n[1] RH-01 compromised — scanning network")
    for port in [22, 80, 443, 445, 3389, 5432, 8080, 8443, 3306, 1433, 6379]:
        send_event("rh-01", "192.168.2.10", port, process="nmap")
        time.sleep(0.2)
    print("    → port scan sent")

    time.sleep(3)

    print("\n[2] RH-01 → Finance DB (lateral movement)")
    send_event("rh-01", "192.168.2.10", 5432, bytes_sent=5000, process="psql")
    time.sleep(2)

    print("\n[3] Finance-01 mutates — contacts unknown server")
    send_event("finance-01", "10.10.10.10", 4444, bytes_sent=50_000_000_000, process="nc")
    time.sleep(2)

    print("\n[4] Finance-01 → exfiltration")
    send_event("finance-01", "10.10.10.10", 443, bytes_sent=50_000_000_000, process="curl")
    time.sleep(2)

    print("\n[*] checking results...")
    try:
        resp = urllib.request.urlopen(f"{CORE_URL}/api/mutations", timeout=5)
        mutations = json.loads(resp.read())
        print(f"\n[✓] {len(mutations)} mutations detected:")
        for m in mutations:
            print(f"    → {m['machine_id']} | score {m['risk_score']:.0%} | {', '.join(m['dimensions'])}")
    except Exception as e:
        print(f"error: {e}")

    try:
        resp = urllib.request.urlopen(f"{CORE_URL}/api/chains", timeout=5)
        chains = json.loads(resp.read())
        if chains:
            print(f"\n[✓] {len(chains)} lateral chains detected:")
            for c in chains:
                print(f"    → {c['path_str']} | score {c['chain_score']:.0%}")
    except Exception as e:
        print(f"error: {e}")

    print("\n[*] attack simulation complete")
    print(f"[*] dashboard: {CORE_URL}/")

def main():
    if SCENARIO == "lateral_movement":
        lateral_movement()
    else:
        print(f"unknown scenario: {SCENARIO}")

if __name__ == "__main__":
    main()
