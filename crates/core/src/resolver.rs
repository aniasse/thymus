use std::net::IpAddr;

/// Resolve an IP to a hostname via reverse DNS (`PTR`). Returns None when there is
/// no `PTR` record, the lookup fails, or the resolver simply echoes the IP back.
/// This is a blocking call — run it off the async runtime (`spawn_blocking`).
pub fn reverse_lookup(ip: IpAddr) -> Option<String> {
    let name = dns_lookup::lookup_addr(&ip).ok()?;
    let name = name.trim_end_matches('.').trim().to_string();

    if name.is_empty() || name == ip.to_string() {
        None
    } else {
        Some(name)
    }
}
