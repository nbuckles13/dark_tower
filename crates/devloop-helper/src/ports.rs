//! Port allocation and port map generation per ADR-0030.
//!
//! Hash-preferred 200-stride scheme with registry file for collision avoidance.
//! Port range: 20000-29999, 50 slots of 200 ports each.

use crate::error::HelperError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Total number of port allocation slots.
const NUM_SLOTS: usize = 50;
/// Ports per slot.
const STRIDE: u16 = 200;
/// Base of the port range.
const PORT_RANGE_START: u16 = 20000;

/// Port offset definitions from ADR-0030.
pub struct PortOffsets;

impl PortOffsets {
    pub const AC_HTTP: u16 = 0;
    pub const GC_HTTP: u16 = 1;
    pub const GC_GRPC: u16 = 2;
    pub const MC_0_HEALTH: u16 = 10;
    pub const MC_0_GRPC: u16 = 11;
    pub const MC_0_WEBTRANSPORT: u16 = 12;
    pub const MC_1_HEALTH: u16 = 13;
    pub const MC_1_GRPC: u16 = 14;
    pub const MC_1_WEBTRANSPORT: u16 = 15;
    pub const MH_0_HEALTH: u16 = 20;
    pub const MH_0_GRPC: u16 = 21;
    pub const MH_0_WEBTRANSPORT: u16 = 22;
    pub const MH_1_HEALTH: u16 = 23;
    pub const MH_1_GRPC: u16 = 24;
    pub const MH_1_WEBTRANSPORT: u16 = 25;
    pub const POSTGRES: u16 = 30;
    pub const PROMETHEUS: u16 = 100;
    pub const GRAFANA: u16 = 101;
    pub const LOKI: u16 = 102;
    pub const K8S_API: u16 = 103;
    /// K8s API on 127.0.0.1 for host-side kubectl (separate from gateway-bound K8S_API)
    pub const K8S_API_HOST: u16 = 104;
}

/// A port allocation for a devloop.
#[derive(Debug, Clone)]
pub struct PortAllocation {
    pub base_port: u16,
    pub slot_index: usize,
}

impl PortAllocation {
    /// Get a specific port by offset.
    pub fn port(&self, offset: u16) -> u16 {
        self.base_port + offset
    }
}

/// Registry entry for a port allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub slug: String,
    pub slot_index: usize,
    pub base_port: u16,
    pub pid: u32,
}

/// Port registry file at `~/.cache/devloop/port-registry.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PortRegistry {
    pub entries: Vec<RegistryEntry>,
}

/// Compute the preferred slot index for a slug by hashing.
pub fn preferred_index(slug: &str) -> usize {
    // Simple hash: djb2
    let mut hash: u64 = 5381;
    for &b in slug.as_bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(b));
    }
    (hash as usize) % NUM_SLOTS
}

/// Check if a PID is alive (simple kill -0 check).
///
/// Used for registry cleanup where false positives from PID recycling
/// have low impact (worst case: a port block stays reserved until the
/// unrelated process exits).
fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Check if a PID is alive AND belongs to a devloop-helper process.
///
/// Used for PID file stale detection where correctness matters more.
/// Checks `/proc/{pid}/cmdline` to guard against PID recycling.
pub fn is_helper_alive(pid: u32) -> bool {
    if !is_process_alive(pid) {
        return false;
    }

    let cmdline_path = format!("/proc/{pid}/cmdline");
    match fs::read_to_string(&cmdline_path) {
        Ok(cmdline) => cmdline.contains("devloop-helper"),
        Err(_) => false,
    }
}

/// Get the cache directory, respecting XDG_CACHE_HOME.
pub fn cache_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("devloop");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("devloop");
    }
    PathBuf::from("/tmp/devloop-cache")
}

/// Registry file path.
pub fn registry_path() -> PathBuf {
    cache_dir().join("port-registry.json")
}

/// Read the port registry, creating it if it doesn't exist.
pub fn read_registry(path: &Path) -> Result<PortRegistry, HelperError> {
    if !path.exists() {
        return Ok(PortRegistry::default());
    }
    let contents = fs::read_to_string(path)?;
    if contents.trim().is_empty() {
        return Ok(PortRegistry::default());
    }
    let registry: PortRegistry = serde_json::from_str(&contents).map_err(|e| {
        HelperError::PortAllocation(format!(
            "failed to parse port registry at {}: {e}",
            path.display()
        ))
    })?;
    Ok(registry)
}

/// Write the port registry to disk.
pub fn write_registry(path: &Path, registry: &PortRegistry) -> Result<(), HelperError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(registry)
        .map_err(|e| HelperError::PortAllocation(format!("failed to serialize registry: {e}")))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// Lock the registry file using flock for concurrent access safety.
pub fn lock_registry(path: &Path) -> Result<fs::File, HelperError> {
    let lock_path = path.with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)?;
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if ret != 0 {
        return Err(HelperError::Io(std::io::Error::last_os_error()));
    }
    Ok(file)
}

/// Allocate ports for a devloop slug.
///
/// Uses hash-preferred index with collision resolution.
/// Cleans up dead entries from the registry before allocating.
pub fn allocate_ports(slug: &str, registry_path: &Path) -> Result<PortAllocation, HelperError> {
    let _lock = lock_registry(registry_path)?;
    let mut registry = read_registry(registry_path)?;

    // Clean up dead entries
    registry.entries.retain(|entry| is_process_alive(entry.pid));

    // Check if this slug already has an allocation
    for entry in &registry.entries {
        if entry.slug == slug {
            return Ok(PortAllocation {
                base_port: entry.base_port,
                slot_index: entry.slot_index,
            });
        }
    }

    // Find a free slot, starting from the preferred index
    let preferred = preferred_index(slug);
    let mut allocated_slots: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for entry in &registry.entries {
        allocated_slots.insert(entry.slot_index);
    }

    let mut chosen_index = None;
    // Try preferred index first, then scan forward
    for offset in 0..NUM_SLOTS {
        let idx = (preferred + offset) % NUM_SLOTS;
        if !allocated_slots.contains(&idx) {
            chosen_index = Some(idx);
            break;
        }
    }

    let slot_index = chosen_index.ok_or_else(|| {
        HelperError::PortAllocation(format!(
            "all {NUM_SLOTS} port slots are in use — cannot allocate ports for '{slug}'"
        ))
    })?;

    let base_port = PORT_RANGE_START + (slot_index as u16) * STRIDE;
    let pid = std::process::id();

    // Register the allocation
    registry.entries.push(RegistryEntry {
        slug: slug.to_string(),
        slot_index,
        base_port,
        pid,
    });

    write_registry(registry_path, &registry)?;

    Ok(PortAllocation {
        base_port,
        slot_index,
    })
}

/// Remove a slug's entry from the port registry.
pub fn deallocate_ports(slug: &str, registry_path: &Path) -> Result<(), HelperError> {
    let _lock = lock_registry(registry_path)?;
    let mut registry = read_registry(registry_path)?;
    registry.entries.retain(|entry| entry.slug != slug);
    write_registry(registry_path, &registry)?;
    Ok(())
}

/// Check if a TCP port is available on localhost.
pub fn is_port_available(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(100)).is_err()
}

/// Verify all critical ports in an allocation are available.
pub fn verify_ports_available(alloc: &PortAllocation) -> Result<(), HelperError> {
    let critical_offsets = [
        ("ac_http", PortOffsets::AC_HTTP),
        ("gc_http", PortOffsets::GC_HTTP),
        ("gc_grpc", PortOffsets::GC_GRPC),
        ("mc_0_health", PortOffsets::MC_0_HEALTH),
        ("mc_0_webtransport", PortOffsets::MC_0_WEBTRANSPORT),
        ("mc_1_webtransport", PortOffsets::MC_1_WEBTRANSPORT),
        ("mh_0_webtransport", PortOffsets::MH_0_WEBTRANSPORT),
        ("mh_1_webtransport", PortOffsets::MH_1_WEBTRANSPORT),
        ("prometheus", PortOffsets::PROMETHEUS),
        ("k8s_api", PortOffsets::K8S_API),
    ];

    for (name, offset) in &critical_offsets {
        let port = alloc.port(*offset);
        if !is_port_available(port) {
            return Err(HelperError::PortAllocation(format!(
                "port {port} ({name}) is already in use"
            )));
        }
    }

    Ok(())
}

/// Port map file for `/tmp/devloop-{slug}/ports.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PortMap {
    pub cluster_name: String,
    pub host: String,
    pub host_fallback: String,
    pub observability_deployed: bool,
    pub ports: PortEntries,
    pub container_urls: HashMap<String, String>,
    pub host_urls: HashMap<String, String>,
    pub created_at: String,
}

/// Named port entries in the port map.
#[derive(Debug, Serialize, Deserialize)]
pub struct PortEntries {
    pub ac_http: u16,
    pub gc_http: u16,
    pub gc_grpc: u16,
    pub mc_0_health: u16,
    pub mc_0_grpc: u16,
    pub mc_0_webtransport: u16,
    pub mc_1_health: u16,
    pub mc_1_grpc: u16,
    pub mc_1_webtransport: u16,
    pub mh_0_health: u16,
    pub mh_0_grpc: u16,
    pub mh_0_webtransport: u16,
    pub mh_1_health: u16,
    pub mh_1_grpc: u16,
    pub mh_1_webtransport: u16,
    pub postgres: u16,
    pub prometheus: u16,
    pub grafana: u16,
    pub loki: u16,
    pub k8s_api: u16,
}

/// Generate the port map from a port allocation.
pub fn generate_port_map(
    alloc: &PortAllocation,
    cluster_name: &str,
    host: &str,
    host_fallback: &str,
    observability_deployed: bool,
) -> PortMap {
    let ports = PortEntries {
        ac_http: alloc.port(PortOffsets::AC_HTTP),
        gc_http: alloc.port(PortOffsets::GC_HTTP),
        gc_grpc: alloc.port(PortOffsets::GC_GRPC),
        mc_0_health: alloc.port(PortOffsets::MC_0_HEALTH),
        mc_0_grpc: alloc.port(PortOffsets::MC_0_GRPC),
        mc_0_webtransport: alloc.port(PortOffsets::MC_0_WEBTRANSPORT),
        mc_1_health: alloc.port(PortOffsets::MC_1_HEALTH),
        mc_1_grpc: alloc.port(PortOffsets::MC_1_GRPC),
        mc_1_webtransport: alloc.port(PortOffsets::MC_1_WEBTRANSPORT),
        mh_0_health: alloc.port(PortOffsets::MH_0_HEALTH),
        mh_0_grpc: alloc.port(PortOffsets::MH_0_GRPC),
        mh_0_webtransport: alloc.port(PortOffsets::MH_0_WEBTRANSPORT),
        mh_1_health: alloc.port(PortOffsets::MH_1_HEALTH),
        mh_1_grpc: alloc.port(PortOffsets::MH_1_GRPC),
        mh_1_webtransport: alloc.port(PortOffsets::MH_1_WEBTRANSPORT),
        postgres: alloc.port(PortOffsets::POSTGRES),
        prometheus: if observability_deployed {
            alloc.port(PortOffsets::PROMETHEUS)
        } else {
            0
        },
        grafana: if observability_deployed {
            alloc.port(PortOffsets::GRAFANA)
        } else {
            0
        },
        loki: if observability_deployed {
            alloc.port(PortOffsets::LOKI)
        } else {
            0
        },
        k8s_api: alloc.port(PortOffsets::K8S_API),
    };

    let mut container_urls = HashMap::new();
    container_urls.insert("ac".to_string(), format!("http://{host}:{}", ports.ac_http));
    container_urls.insert("gc".to_string(), format!("http://{host}:{}", ports.gc_http));
    if observability_deployed {
        container_urls.insert(
            "prometheus".to_string(),
            format!("http://{host}:{}", ports.prometheus),
        );
        container_urls.insert(
            "grafana".to_string(),
            format!("http://{host}:{}", ports.grafana),
        );
        container_urls.insert("loki".to_string(), format!("http://{host}:{}", ports.loki));
    }

    let mut host_urls = HashMap::new();
    host_urls.insert(
        "ac".to_string(),
        format!("http://localhost:{}", ports.ac_http),
    );
    host_urls.insert(
        "gc".to_string(),
        format!("http://localhost:{}", ports.gc_http),
    );
    if observability_deployed {
        host_urls.insert(
            "prometheus".to_string(),
            format!("http://localhost:{}", ports.prometheus),
        );
        host_urls.insert(
            "grafana".to_string(),
            format!("http://localhost:{}", ports.grafana),
        );
        host_urls.insert(
            "loki".to_string(),
            format!("http://localhost:{}", ports.loki),
        );
    }

    PortMap {
        cluster_name: cluster_name.to_string(),
        host: host.to_string(),
        host_fallback: host_fallback.to_string(),
        observability_deployed,
        ports,
        container_urls,
        host_urls,
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    }
}

/// Write the port map to a JSON file.
pub fn write_port_map(path: &Path, port_map: &PortMap) -> Result<(), HelperError> {
    let json = serde_json::to_string_pretty(port_map)
        .map_err(|e| HelperError::PortAllocation(format!("failed to serialize port map: {e}")))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// Generate environment variables for kind-config.yaml.tmpl template substitution.
pub fn template_env_vars(alloc: &PortAllocation, host_gateway_ip: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("HOST_GATEWAY_IP".to_string(), host_gateway_ip.to_string());
    vars.insert(
        "HOST_PORT_AC_HTTP".to_string(),
        alloc.port(PortOffsets::AC_HTTP).to_string(),
    );
    vars.insert(
        "HOST_PORT_GC_HTTP".to_string(),
        alloc.port(PortOffsets::GC_HTTP).to_string(),
    );
    vars.insert(
        "HOST_PORT_GC_GRPC".to_string(),
        alloc.port(PortOffsets::GC_GRPC).to_string(),
    );
    vars.insert(
        "HOST_PORT_MC_0_HEALTH".to_string(),
        alloc.port(PortOffsets::MC_0_HEALTH).to_string(),
    );
    vars.insert(
        "HOST_PORT_MC_0_GRPC".to_string(),
        alloc.port(PortOffsets::MC_0_GRPC).to_string(),
    );
    vars.insert(
        "HOST_PORT_MC_0_WEBTRANSPORT".to_string(),
        alloc.port(PortOffsets::MC_0_WEBTRANSPORT).to_string(),
    );
    vars.insert(
        "HOST_PORT_MC_1_HEALTH".to_string(),
        alloc.port(PortOffsets::MC_1_HEALTH).to_string(),
    );
    vars.insert(
        "HOST_PORT_MC_1_GRPC".to_string(),
        alloc.port(PortOffsets::MC_1_GRPC).to_string(),
    );
    vars.insert(
        "HOST_PORT_MC_1_WEBTRANSPORT".to_string(),
        alloc.port(PortOffsets::MC_1_WEBTRANSPORT).to_string(),
    );
    vars.insert(
        "HOST_PORT_MH_0_HEALTH".to_string(),
        alloc.port(PortOffsets::MH_0_HEALTH).to_string(),
    );
    vars.insert(
        "HOST_PORT_MH_0_GRPC".to_string(),
        alloc.port(PortOffsets::MH_0_GRPC).to_string(),
    );
    vars.insert(
        "HOST_PORT_MH_0_WEBTRANSPORT".to_string(),
        alloc.port(PortOffsets::MH_0_WEBTRANSPORT).to_string(),
    );
    vars.insert(
        "HOST_PORT_MH_1_HEALTH".to_string(),
        alloc.port(PortOffsets::MH_1_HEALTH).to_string(),
    );
    vars.insert(
        "HOST_PORT_MH_1_GRPC".to_string(),
        alloc.port(PortOffsets::MH_1_GRPC).to_string(),
    );
    vars.insert(
        "HOST_PORT_MH_1_WEBTRANSPORT".to_string(),
        alloc.port(PortOffsets::MH_1_WEBTRANSPORT).to_string(),
    );
    vars.insert(
        "HOST_PORT_PROMETHEUS".to_string(),
        alloc.port(PortOffsets::PROMETHEUS).to_string(),
    );
    vars.insert(
        "HOST_PORT_GRAFANA".to_string(),
        alloc.port(PortOffsets::GRAFANA).to_string(),
    );
    vars.insert(
        "HOST_PORT_LOKI".to_string(),
        alloc.port(PortOffsets::LOKI).to_string(),
    );
    vars.insert(
        "HOST_PORT_K8S_API".to_string(),
        alloc.port(PortOffsets::K8S_API).to_string(),
    );
    vars.insert(
        "HOST_PORT_K8S_API_HOST".to_string(),
        alloc.port(PortOffsets::K8S_API_HOST).to_string(),
    );
    vars
}

/// Substitute `${VAR}` placeholders in a template string.
pub fn substitute_template(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("${{{key}}}");
        result = result.replace(&placeholder, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preferred_index_deterministic() {
        let idx1 = preferred_index("my-task");
        let idx2 = preferred_index("my-task");
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn test_preferred_index_in_range() {
        let slugs = [
            "task-1",
            "task-2",
            "my-feature",
            "fix-bug-123",
            "a",
            "very-long-slug-name-that-is-still-valid",
        ];
        for slug in &slugs {
            let idx = preferred_index(slug);
            assert!(
                idx < NUM_SLOTS,
                "index {idx} out of range for slug '{slug}'"
            );
        }
    }

    #[test]
    fn test_preferred_index_distribution() {
        // Different slugs should generally map to different indices
        let mut indices = std::collections::HashSet::new();
        for i in 0..50 {
            indices.insert(preferred_index(&format!("task-{i}")));
        }
        // With 50 slugs and 50 slots, we expect reasonable distribution
        // (at least 20 unique indices due to birthday paradox)
        assert!(
            indices.len() >= 15,
            "poor hash distribution: only {} unique indices from 50 slugs",
            indices.len()
        );
    }

    #[test]
    fn test_port_allocation_offsets() {
        let alloc = PortAllocation {
            base_port: 24200,
            slot_index: 21,
        };
        assert_eq!(alloc.port(PortOffsets::AC_HTTP), 24200);
        assert_eq!(alloc.port(PortOffsets::GC_HTTP), 24201);
        assert_eq!(alloc.port(PortOffsets::GC_GRPC), 24202);
        assert_eq!(alloc.port(PortOffsets::MC_0_HEALTH), 24210);
        assert_eq!(alloc.port(PortOffsets::MC_0_WEBTRANSPORT), 24212);
        assert_eq!(alloc.port(PortOffsets::MC_1_WEBTRANSPORT), 24215);
        assert_eq!(alloc.port(PortOffsets::MH_0_WEBTRANSPORT), 24222);
        assert_eq!(alloc.port(PortOffsets::MH_1_WEBTRANSPORT), 24225);
        assert_eq!(alloc.port(PortOffsets::PROMETHEUS), 24300);
        assert_eq!(alloc.port(PortOffsets::GRAFANA), 24301);
        assert_eq!(alloc.port(PortOffsets::LOKI), 24302);
        assert_eq!(alloc.port(PortOffsets::K8S_API), 24303);
    }

    #[test]
    fn test_registry_read_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-registry.json");

        // Empty registry
        let registry = read_registry(&path).unwrap();
        assert!(registry.entries.is_empty());

        // Write and read back
        let mut registry = PortRegistry::default();
        registry.entries.push(RegistryEntry {
            slug: "test-slug".to_string(),
            slot_index: 5,
            base_port: 21000,
            pid: 12345,
        });
        write_registry(&path, &registry).unwrap();

        let registry2 = read_registry(&path).unwrap();
        assert_eq!(registry2.entries.len(), 1);
        assert_eq!(registry2.entries[0].slug, "test-slug");
        assert_eq!(registry2.entries[0].base_port, 21000);
    }

    #[test]
    fn test_allocate_ports_empty_registry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-registry.json");

        let alloc = allocate_ports("test-slug", &path).unwrap();
        let expected_index = preferred_index("test-slug");
        assert_eq!(alloc.slot_index, expected_index);
        assert_eq!(
            alloc.base_port,
            PORT_RANGE_START + (expected_index as u16) * STRIDE
        );
    }

    #[test]
    fn test_allocate_ports_collision_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-registry.json");

        // Pre-populate registry with an entry at the preferred index for "test-slug"
        let preferred = preferred_index("test-slug");
        let mut registry = PortRegistry::default();
        registry.entries.push(RegistryEntry {
            slug: "other-slug".to_string(),
            slot_index: preferred,
            base_port: PORT_RANGE_START + (preferred as u16) * STRIDE,
            pid: std::process::id(), // Current PID = alive
        });
        write_registry(&path, &registry).unwrap();

        let alloc = allocate_ports("test-slug", &path).unwrap();
        // Should get next free slot, not the preferred one
        assert_ne!(alloc.slot_index, preferred);
        let expected_next = (preferred + 1) % NUM_SLOTS;
        assert_eq!(alloc.slot_index, expected_next);
    }

    #[test]
    fn test_allocate_ports_existing_slug_reuse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-registry.json");

        // First allocation
        let alloc1 = allocate_ports("test-slug", &path).unwrap();
        // Second allocation for same slug should return same ports
        let alloc2 = allocate_ports("test-slug", &path).unwrap();
        assert_eq!(alloc1.base_port, alloc2.base_port);
        assert_eq!(alloc1.slot_index, alloc2.slot_index);
    }

    #[test]
    fn test_allocate_ports_dead_pid_reclaim() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-registry.json");

        let preferred = preferred_index("test-slug");
        let mut registry = PortRegistry::default();
        registry.entries.push(RegistryEntry {
            slug: "dead-slug".to_string(),
            slot_index: preferred,
            base_port: PORT_RANGE_START + (preferred as u16) * STRIDE,
            pid: 999_999_999, // Definitely not alive
        });
        write_registry(&path, &registry).unwrap();

        // Dead PID should be cleaned up, preferred index should be available
        let alloc = allocate_ports("test-slug", &path).unwrap();
        assert_eq!(alloc.slot_index, preferred);
    }

    #[test]
    fn test_allocate_ports_saturation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-registry.json");

        // Fill all slots with live PIDs (current process)
        let mut registry = PortRegistry::default();
        for i in 0..NUM_SLOTS {
            registry.entries.push(RegistryEntry {
                slug: format!("slug-{i}"),
                slot_index: i,
                base_port: PORT_RANGE_START + (i as u16) * STRIDE,
                pid: std::process::id(),
            });
        }
        write_registry(&path, &registry).unwrap();

        let result = allocate_ports("new-slug", &path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("all 50 port slots are in use"), "got: {err}");
    }

    #[test]
    fn test_deallocate_ports() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-registry.json");

        allocate_ports("test-slug", &path).unwrap();
        let registry = read_registry(&path).unwrap();
        assert_eq!(registry.entries.len(), 1);

        deallocate_ports("test-slug", &path).unwrap();
        let registry = read_registry(&path).unwrap();
        assert!(registry.entries.is_empty());
    }

    #[test]
    fn test_generate_port_map() {
        let alloc = PortAllocation {
            base_port: 24200,
            slot_index: 21,
        };
        let port_map = generate_port_map(
            &alloc,
            "devloop-td-42",
            "host.containers.internal",
            "172.17.0.1",
            true,
        );

        assert_eq!(port_map.cluster_name, "devloop-td-42");
        assert_eq!(port_map.host, "host.containers.internal");
        assert!(port_map.observability_deployed);
        assert_eq!(port_map.ports.ac_http, 24200);
        assert_eq!(port_map.ports.gc_http, 24201);
        assert_eq!(port_map.ports.prometheus, 24300);
        assert_eq!(port_map.ports.k8s_api, 24303);

        // Check container_urls include all observability endpoints
        assert!(port_map.container_urls.contains_key("loki"));
        assert!(port_map.container_urls.contains_key("prometheus"));
        assert!(port_map.container_urls.contains_key("grafana"));
        assert!(port_map.container_urls.contains_key("ac"));
        assert!(port_map.container_urls.contains_key("gc"));

        // Check host_urls include all observability endpoints (ADR-0030)
        assert!(port_map.host_urls.contains_key("prometheus"));
        assert!(port_map.host_urls.contains_key("grafana"));
        assert!(port_map.host_urls.contains_key("loki"));
    }

    #[test]
    fn test_generate_port_map_no_observability() {
        let alloc = PortAllocation {
            base_port: 24200,
            slot_index: 21,
        };
        let port_map = generate_port_map(&alloc, "devloop-test", "localhost", "localhost", false);

        assert!(!port_map.observability_deployed);
        assert_eq!(port_map.ports.prometheus, 0);
        assert_eq!(port_map.ports.grafana, 0);
        assert_eq!(port_map.ports.loki, 0);
        // Observability URLs should not be present
        assert!(!port_map.container_urls.contains_key("prometheus"));
        assert!(!port_map.host_urls.contains_key("grafana"));
    }

    #[test]
    fn test_substitute_template() {
        let template = "name: ${CLUSTER_NAME}\nport: ${HOST_PORT_AC_HTTP}";
        let mut vars = HashMap::new();
        vars.insert("CLUSTER_NAME".to_string(), "devloop-test".to_string());
        vars.insert("HOST_PORT_AC_HTTP".to_string(), "24200".to_string());

        let result = substitute_template(template, &vars);
        assert_eq!(result, "name: devloop-test\nport: 24200");
    }

    #[test]
    fn test_substitute_template_preserves_unknown_vars() {
        let template = "name: ${CLUSTER_NAME}\nunknown: ${UNKNOWN_VAR}";
        let mut vars = HashMap::new();
        vars.insert("CLUSTER_NAME".to_string(), "devloop-test".to_string());

        let result = substitute_template(template, &vars);
        assert!(result.contains("${UNKNOWN_VAR}"));
    }

    #[test]
    fn test_template_env_vars_completeness() {
        let alloc = PortAllocation {
            base_port: 20000,
            slot_index: 0,
        };
        let vars = template_env_vars(&alloc, "10.255.255.254");

        // Verify all expected vars are present
        let expected_keys = [
            "HOST_GATEWAY_IP",
            "HOST_PORT_AC_HTTP",
            "HOST_PORT_GC_HTTP",
            "HOST_PORT_GC_GRPC",
            "HOST_PORT_MC_0_HEALTH",
            "HOST_PORT_MC_0_GRPC",
            "HOST_PORT_MC_0_WEBTRANSPORT",
            "HOST_PORT_MC_1_HEALTH",
            "HOST_PORT_MC_1_GRPC",
            "HOST_PORT_MC_1_WEBTRANSPORT",
            "HOST_PORT_MH_0_HEALTH",
            "HOST_PORT_MH_0_GRPC",
            "HOST_PORT_MH_0_WEBTRANSPORT",
            "HOST_PORT_MH_1_HEALTH",
            "HOST_PORT_MH_1_GRPC",
            "HOST_PORT_MH_1_WEBTRANSPORT",
            "HOST_PORT_PROMETHEUS",
            "HOST_PORT_GRAFANA",
            "HOST_PORT_LOKI",
            "HOST_PORT_K8S_API",
            "HOST_PORT_K8S_API_HOST",
        ];

        for key in &expected_keys {
            assert!(vars.contains_key(*key), "missing template var: {key}");
        }
    }

    #[test]
    fn test_write_port_map() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ports.json");

        let alloc = PortAllocation {
            base_port: 24200,
            slot_index: 21,
        };
        let port_map = generate_port_map(
            &alloc,
            "devloop-test",
            "host.containers.internal",
            "172.17.0.1",
            true,
        );
        write_port_map(&path, &port_map).unwrap();

        // Read back and verify
        let contents = fs::read_to_string(&path).unwrap();
        let parsed: PortMap = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.cluster_name, "devloop-test");
        assert_eq!(parsed.ports.ac_http, 24200);

        // Check permissions
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}
