//! RegisterMH endpoint allowlists — arm 0 of the GC registration rule
//! (ADR-0039 §7; the rule is stated once above `RegisterMHRequest` in
//! `proto/dark_tower/internal/v1/internal.proto`).
//!
//! Two zones, each config-driven (keys in `config.rs`, all REQUIRED):
//!
//! - **Client-facing** (`webtransport_endpoint`): scheme `https` only (fixed
//!   here, deliberately NOT configurable), host in `GC_MH_WT_ALLOWED_HOSTS`
//!   (exact entries or `.suffix` entries), port in `GC_MH_WT_ALLOWED_PORTS`.
//! - **Internal** (`grpc_endpoint`): scheme in `GC_MH_GRPC_ALLOWED_SCHEMES`,
//!   host an IP literal inside `GC_MH_GRPC_ALLOWED_CIDRS`, port in
//!   `GC_MH_GRPC_ALLOWED_PORTS`.
//!
//! # Canonical form, not normalisation
//!
//! GC stores endpoints as received and compares them as exact strings (arms 1
//! and 2 of the rule). So an endpoint must already be in its ONE canonical
//! spelling, and any other spelling is rejected rather than normalised —
//! otherwise `https://H:443` and `https://h:443` (or `https://h:443/`) would be
//! two live holders of one address. Grammar: `scheme "://" host ":" port`,
//! ASCII printable only, with
//! - no userinfo, path (not even a trailing `/`), query or fragment;
//! - a lowercase scheme token;
//! - a lowercase DNS name, a canonical IPv4 literal, or a bracketed canonical
//!   IPv6 literal (no zone id, no IPv4-mapped form). "Canonical IP" means the
//!   text equals `IpAddr::to_string()` of itself, which rejects leading zeros,
//!   short forms and alternative IPv6 spellings;
//! - an explicit decimal port in 1..=65535 without leading zeros (no implicit
//!   default port).
//!
//! `url::Url` is deliberately not used: it treats `grpc://` as an opaque
//! scheme (no IP classification) and normalises `https` hosts, so the parsed
//! value would differ from the stored string the rule compares.
//!
//! # What this is not
//!
//! The allowlist bounds where MC will send its credential (MC dials
//! `grpc_endpoint` with its bearer). It is NOT peer authentication and does not
//! close S6 (`docs/TODO.md` §Media Path Obligations).

use ipnet::IpNet;
use std::collections::BTreeSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// The only client-facing (WebTransport) scheme. Fixed, not configurable:
/// browsers dial it, and it is https by definition.
pub const WT_SCHEME: &str = "https";

/// Which registration field an endpoint came from (its zone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointField {
    /// `RegisterMHRequest.webtransport_endpoint` (client-facing zone).
    WebTransport,
    /// `RegisterMHRequest.grpc_endpoint` (internal zone).
    Grpc,
}

impl EndpointField {
    /// The proto field name.
    #[must_use]
    pub fn field_name(self) -> &'static str {
        match self {
            EndpointField::WebTransport => "webtransport_endpoint",
            EndpointField::Grpc => "grpc_endpoint",
        }
    }
}

/// Which part of an endpoint failed arm 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectedComponent {
    /// Not in the canonical grammar at all.
    Syntax,
    /// Scheme not allowed in the zone.
    Scheme,
    /// Host not allowed in the zone.
    Host,
    /// Port not allowed in the zone.
    Port,
}

impl RejectedComponent {
    /// Stable lowercase name (used in the Status message and logs).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RejectedComponent::Syntax => "syntax",
            RejectedComponent::Scheme => "scheme",
            RejectedComponent::Host => "host",
            RejectedComponent::Port => "port",
        }
    }
}

/// An arm-0 rejection: the field and the failed component. Never carries the
/// configured allowlist (it is not echoed back to the caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointRejection {
    /// The endpoint's field (zone).
    pub field: EndpointField,
    /// The failed component.
    pub component: RejectedComponent,
}

impl fmt::Display for EndpointRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} not allowed",
            self.field.field_name(),
            self.component.as_str()
        )
    }
}

/// A parsed endpoint host.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Host {
    /// An IP literal (canonical text).
    Ip(IpAddr),
    /// A lowercase DNS name.
    Name(String),
}

/// One `GC_MH_WT_ALLOWED_HOSTS` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostEntry {
    /// Exact host (DNS name or IP literal; IPs compare as parsed `IpAddr`).
    Exact(Host),
    /// `.suffix`: matches a DNS name ending in it, on a label boundary (the
    /// leading `.` is part of the stored value), never the apex and NEVER an
    /// IP-literal host.
    Suffix(String),
}

/// A parsed endpoint in canonical form.
#[derive(Debug)]
struct Endpoint<'a> {
    scheme: &'a str,
    host: Host,
    port: u16,
}

/// The per-zone RegisterMH endpoint allowlists, parsed and validated at
/// startup (`Config::from_vars`). Not secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MhEndpointAllowlist {
    wt_hosts: Vec<HostEntry>,
    wt_ports: BTreeSet<u16>,
    grpc_schemes: Vec<String>,
    grpc_cidrs: Vec<IpNet>,
    grpc_ports: BTreeSet<u16>,
}

/// Parsed `GC_MH_WT_ALLOWED_HOSTS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WtHosts(Vec<HostEntry>);
/// A parsed port set (`GC_MH_WT_ALLOWED_PORTS`, `GC_MH_GRPC_ALLOWED_PORTS`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSet(BTreeSet<u16>);
/// Parsed `GC_MH_GRPC_ALLOWED_SCHEMES`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeSet(Vec<String>);
/// Parsed `GC_MH_GRPC_ALLOWED_CIDRS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidrSet(Vec<IpNet>);

impl MhEndpointAllowlist {
    /// Assemble from the five parsed keys.
    #[must_use]
    pub fn new(
        wt_hosts: WtHosts,
        wt_ports: PortSet,
        grpc_schemes: SchemeSet,
        grpc_cidrs: CidrSet,
        grpc_ports: PortSet,
    ) -> Self {
        Self {
            wt_hosts: wt_hosts.0,
            wt_ports: wt_ports.0,
            grpc_schemes: grpc_schemes.0,
            grpc_cidrs: grpc_cidrs.0,
            grpc_ports: grpc_ports.0,
        }
    }

    /// Arm 0: validate both endpoints against their zone. The client-facing
    /// endpoint is checked first.
    ///
    /// # Errors
    ///
    /// The first failing field and component.
    pub fn check(&self, webtransport: &str, grpc: &str) -> Result<(), EndpointRejection> {
        self.check_wt(webtransport)
            .map_err(|component| EndpointRejection {
                field: EndpointField::WebTransport,
                component,
            })?;
        self.check_grpc(grpc)
            .map_err(|component| EndpointRejection {
                field: EndpointField::Grpc,
                component,
            })
    }

    fn check_wt(&self, endpoint: &str) -> Result<(), RejectedComponent> {
        let ep = parse_endpoint(endpoint).ok_or(RejectedComponent::Syntax)?;
        if ep.scheme != WT_SCHEME {
            return Err(RejectedComponent::Scheme);
        }
        if !self
            .wt_hosts
            .iter()
            .any(|e| host_entry_matches(e, &ep.host))
        {
            return Err(RejectedComponent::Host);
        }
        if !self.wt_ports.contains(&ep.port) {
            return Err(RejectedComponent::Port);
        }
        Ok(())
    }

    fn check_grpc(&self, endpoint: &str) -> Result<(), RejectedComponent> {
        let ep = parse_endpoint(endpoint).ok_or(RejectedComponent::Syntax)?;
        // Whole-token exact equality, never a prefix match.
        if !self.grpc_schemes.iter().any(|s| s == ep.scheme) {
            return Err(RejectedComponent::Scheme);
        }
        // An IP literal only: a DNS name would defeat the CIDR check
        // (rebinding / resolver control), so it is rejected outright.
        let Host::Ip(ip) = ep.host else {
            return Err(RejectedComponent::Host);
        };
        if !self.grpc_cidrs.iter().any(|net| net.contains(&ip)) {
            return Err(RejectedComponent::Host);
        }
        if !self.grpc_ports.contains(&ep.port) {
            return Err(RejectedComponent::Port);
        }
        Ok(())
    }
}

fn host_entry_matches(entry: &HostEntry, host: &Host) -> bool {
    match (entry, host) {
        (HostEntry::Exact(want), got) => want == got,
        // The stored suffix starts with '.', so `ends_with` is a label-boundary
        // match and excludes the apex. Never matches an IP host.
        (HostEntry::Suffix(suffix), Host::Name(name)) => name.ends_with(suffix.as_str()),
        (HostEntry::Suffix(_), Host::Ip(_)) => false,
    }
}

// ============================================================================
// Endpoint grammar
// ============================================================================

/// Parse an endpoint in canonical form; `None` for anything else.
fn parse_endpoint(s: &str) -> Option<Endpoint<'_>> {
    // ASCII printable, no whitespace or control characters.
    if !s.bytes().all(|b| b.is_ascii_graphic()) {
        return None;
    }
    let (scheme, rest) = s.split_once("://")?;
    if !is_scheme_token(scheme) {
        return None;
    }
    // No userinfo, path (incl. a trailing '/'), query or fragment.
    if rest.contains(['/', '?', '#', '@']) {
        return None;
    }
    let (host, port) = if let Some(after_bracket) = rest.strip_prefix('[') {
        let (v6, tail) = after_bracket.split_once(']')?;
        let port = tail.strip_prefix(':')?;
        (Host::Ip(IpAddr::V6(parse_canonical_v6(v6)?)), port)
    } else {
        let (host, port) = rest.rsplit_once(':')?;
        if host.contains(':') {
            return None; // unbracketed IPv6 or a stray ':'
        }
        (parse_host_unbracketed(host)?, port)
    };
    Some(Endpoint {
        scheme,
        host,
        port: parse_port(port)?,
    })
}

/// RFC 3986 scheme token, lowercase only: `[a-z][a-z0-9+.-]*`.
fn is_scheme_token(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b))
}

/// A canonical IPv4 literal or a lowercase DNS name.
fn parse_host_unbracketed(s: &str) -> Option<Host> {
    if let Ok(v4) = Ipv4Addr::from_str(s) {
        return (v4.to_string() == s).then_some(Host::Ip(IpAddr::V4(v4)));
    }
    is_dns_name(s).then(|| Host::Name(s.to_string()))
}

/// A canonical, non-IPv4-mapped IPv6 literal (no zone id: `from_str` rejects
/// `%`).
fn parse_canonical_v6(s: &str) -> Option<Ipv6Addr> {
    let v6 = Ipv6Addr::from_str(s).ok()?;
    if v6.to_string() != s || v6.to_ipv4_mapped().is_some() {
        return None;
    }
    Some(v6)
}

/// Lowercase DNS name: labels of `[a-z0-9-]` (1..=63, no leading/trailing
/// `-`), total <= 253, no trailing dot, and a non-numeric last label (so no
/// IP-shaped name such as `1.2.3` is a "name").
fn is_dns_name(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    let labels: Vec<&str> = s.split('.').collect();
    let label_ok = |l: &&str| {
        !l.is_empty()
            && l.len() <= 63
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    labels.iter().all(label_ok)
        && labels
            .last()
            .is_some_and(|l| !l.bytes().all(|b| b.is_ascii_digit()))
}

/// Decimal port 1..=65535, no sign, no leading zeros.
fn parse_port(s: &str) -> Option<u16> {
    if s.is_empty() || s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u16>().ok().filter(|p| *p != 0)
}

// ============================================================================
// Config parsing (one function per key shape; `config.rs` names the key)
// ============================================================================

/// Split a comma-separated config list. An empty value or an empty element
/// (e.g. a trailing comma) is an error: never "allow all".
fn split_list(raw: &str) -> Result<Vec<&str>, String> {
    if raw.trim().is_empty() {
        return Err("must be a non-empty comma-separated list".to_string());
    }
    raw.split(',')
        .map(|e| {
            let e = e.trim();
            if e.is_empty() {
                Err("empty list entry (stray comma)".to_string())
            } else {
                Ok(e)
            }
        })
        .collect()
}

/// Parse `GC_MH_WT_ALLOWED_HOSTS`.
///
/// # Errors
///
/// The offending entry, for an empty list, a wildcard (`*`, `.`), a `.suffix`
/// that is not a DNS name (incl. an all-numeric one), or an exact entry that
/// is neither a canonical IP literal nor a lowercase DNS name.
pub fn parse_wt_hosts(raw: &str) -> Result<WtHosts, String> {
    split_list(raw)?
        .into_iter()
        .map(|entry| {
            let parsed = if let Some(name) = entry.strip_prefix('.') {
                is_dns_name(name).then(|| HostEntry::Suffix(entry.to_string()))
            } else if let Some(inner) = entry.strip_prefix('[').and_then(|e| e.strip_suffix(']')) {
                parse_canonical_v6(inner).map(|v6| HostEntry::Exact(Host::Ip(IpAddr::V6(v6))))
            } else if entry.contains(':') {
                parse_canonical_v6(entry).map(|v6| HostEntry::Exact(Host::Ip(IpAddr::V6(v6))))
            } else {
                parse_host_unbracketed(entry).map(HostEntry::Exact)
            };
            parsed.ok_or_else(|| {
                format!(
                    "entry '{entry}' is not a lowercase DNS name, a canonical IP literal, \
                     or a '.suffix' DNS name"
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(WtHosts)
}

/// Parse a port set (`GC_MH_WT_ALLOWED_PORTS`, `GC_MH_GRPC_ALLOWED_PORTS`).
///
/// # Errors
///
/// The offending entry, for an empty list or a value outside 1..=65535 (or
/// with a sign or leading zeros).
pub fn parse_port_set(raw: &str) -> Result<PortSet, String> {
    split_list(raw)?
        .into_iter()
        .map(|entry| {
            parse_port(entry).ok_or_else(|| format!("entry '{entry}' is not a port in 1..=65535"))
        })
        .collect::<Result<BTreeSet<_>, _>>()
        .map(PortSet)
}

/// Parse `GC_MH_GRPC_ALLOWED_SCHEMES`.
///
/// Grammar only (a lowercase RFC 3986 scheme token), NOT a closed in-code
/// list: ADR-0039 §7 says the scheme flips "by config only" to a TLS scheme
/// when S6 mTLS lands, spelling decided then. Today's value is `grpc`, which
/// means **cleartext h2c, unauthenticated**: no TLS and no peer authentication
/// of the MH server, so MC's bearer crosses that channel in cleartext (S6).
///
/// # Errors
///
/// The offending entry, for an empty list or a non-token entry (incl. `*`).
pub fn parse_scheme_set(raw: &str) -> Result<SchemeSet, String> {
    split_list(raw)?
        .into_iter()
        .map(|entry| {
            if is_scheme_token(entry) {
                Ok(entry.to_string())
            } else {
                Err(format!(
                    "entry '{entry}' is not a lowercase URI scheme token"
                ))
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(SchemeSet)
}

/// Parse `GC_MH_GRPC_ALLOWED_CIDRS`.
///
/// # Errors
///
/// The offending entry, for an empty list, a non-CIDR, a CIDR with host bits
/// set (a typo must not silently widen the range), or a `/0` wildcard.
pub fn parse_cidr_set(raw: &str) -> Result<CidrSet, String> {
    split_list(raw)?
        .into_iter()
        .map(|entry| {
            let net = IpNet::from_str(entry)
                .map_err(|_| format!("entry '{entry}' is not a CIDR (address/prefix)"))?;
            if net.trunc() != net {
                return Err(format!(
                    "entry '{entry}' has host bits set (did you mean {}?)",
                    net.trunc()
                ));
            }
            if net.prefix_len() == 0 {
                return Err(format!("entry '{entry}' is a /0 wildcard"));
            }
            Ok(net)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(CidrSet)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WT zone: exact gateway IP + `.mh.example.com`, ports 4440/4441.
    /// gRPC zone: `grpc`, 192.168.0.0/16, port 50053.
    fn allowlist() -> MhEndpointAllowlist {
        MhEndpointAllowlist::new(
            parse_wt_hosts("172.18.0.1,.mh.example.com,[2001:db8::1]").unwrap(),
            parse_port_set("4440,4441").unwrap(),
            parse_scheme_set("grpc").unwrap(),
            parse_cidr_set("192.168.0.0/16").unwrap(),
            parse_port_set("50053").unwrap(),
        )
    }

    const GOOD_WT: &str = "https://172.18.0.1:4440";
    const GOOD_GRPC: &str = "grpc://192.168.3.7:50053";

    fn wt(ep: &str) -> Result<(), EndpointRejection> {
        allowlist().check(ep, GOOD_GRPC)
    }
    fn grpc(ep: &str) -> Result<(), EndpointRejection> {
        allowlist().check(GOOD_WT, ep)
    }
    fn rej(field: EndpointField, component: RejectedComponent) -> Result<(), EndpointRejection> {
        Err(EndpointRejection { field, component })
    }
    use EndpointField::{Grpc as G, WebTransport as W};
    use RejectedComponent::{Host as H, Port as P, Scheme as S, Syntax as X};

    #[test]
    fn positive_pair_is_accepted() {
        assert_eq!(allowlist().check(GOOD_WT, GOOD_GRPC), Ok(()));
    }

    /// Each negative sits next to its positive twin.
    #[test]
    fn wt_zone_table() {
        let cases: &[(&str, Result<(), EndpointRejection>, &str)] = &[
            ("https://172.18.0.1:4441", Ok(()), "second port"),
            ("https://a.mh.example.com:4440", Ok(()), "suffix match"),
            (
                "https://a.b.mh.example.com:4440",
                Ok(()),
                "deep suffix match",
            ),
            (
                "https://mh.example.com:4440",
                rej(W, H),
                "suffix never matches the apex",
            ),
            (
                "https://evilmh.example.com:4440",
                rej(W, H),
                "suffix needs a label boundary",
            ),
            ("https://[2001:db8::1]:4440", Ok(()), "exact IPv6 entry"),
            (
                "https://[2001:db8:0::1]:4440",
                rej(W, X),
                "non-canonical IPv6",
            ),
            (
                "http://172.18.0.1:4440",
                rej(W, S),
                "http is not allowed for WT",
            ),
            (
                "grpc://172.18.0.1:4440",
                rej(W, S),
                "grpc scheme in WT zone",
            ),
            ("HTTPS://172.18.0.1:4440", rej(W, X), "uppercase scheme"),
            ("https://172.18.0.2:4440", rej(W, H), "host not listed"),
            ("https://172.18.0.1:4442", rej(W, P), "port not listed"),
            ("https://172.18.0.1", rej(W, X), "implicit port"),
            ("https://172.18.0.1:4440/", rej(W, X), "trailing slash"),
            ("https://172.18.0.1:4440/x", rej(W, X), "path"),
            ("https://172.18.0.1:4440?q", rej(W, X), "query"),
            ("https://172.18.0.1:4440#f", rej(W, X), "fragment"),
            ("https://u:p@172.18.0.1:4440", rej(W, X), "userinfo"),
            ("https://172.18.0.1:04440", rej(W, X), "leading-zero port"),
            ("https://172.18.0.1:0", rej(W, X), "port 0"),
            ("https://172.18.0.1:65536", rej(W, X), "port out of range"),
            ("https://172.018.0.1:4440", rej(W, X), "non-canonical IPv4"),
            ("https://A.mh.example.com:4440", rej(W, X), "uppercase host"),
            ("https://a.mh.example.com.:4440", rej(W, X), "trailing dot"),
            ("https://172.18.0.1 :4440", rej(W, X), "whitespace"),
            ("https://172.18.0.1:4440\n", rej(W, X), "control character"),
            ("", rej(W, X), "empty"),
        ];
        for (ep, want, why) in cases {
            assert_eq!(wt(ep), *want, "{why}: {ep:?}");
        }
    }

    #[test]
    fn grpc_zone_table() {
        let cases: &[(&str, Result<(), EndpointRejection>, &str)] = &[
            ("grpc://192.168.255.254:50053", Ok(()), "inside CIDR"),
            ("grpc://10.0.0.1:50053", rej(G, H), "outside CIDR"),
            (
                "grpc://mh.example.com:50053",
                rej(G, H),
                "hostname rejected outright",
            ),
            ("https://192.168.3.7:50053", rej(G, S), "scheme not listed"),
            (
                "grpcs://192.168.3.7:50053",
                rej(G, S),
                "prefix-sharing scheme not listed",
            ),
            ("grpc://192.168.3.7:50054", rej(G, P), "port not listed"),
            ("grpc://192.168.3.7", rej(G, X), "implicit port"),
            ("grpc://192.168.3.7:50053/", rej(G, X), "trailing slash"),
            (
                "grpc://[::ffff:192.168.3.7]:50053",
                rej(G, X),
                "IPv4-mapped IPv6",
            ),
            ("grpc://[fe80::1%eth0]:50053", rej(G, X), "zone id"),
            ("grpc://0300.0250.3.7:50053", rej(G, X), "octal IPv4"),
            (
                "grpc://192.168.3:50053",
                rej(G, X),
                "short IPv4 (numeric last label)",
            ),
        ];
        for (ep, want, why) in cases {
            assert_eq!(grpc(ep), *want, "{why}: {ep:?}");
        }
    }

    /// Cross-zone distinguishing inputs: an address valid in one zone is
    /// rejected in the other.
    #[test]
    fn cross_zone_inputs_rejected() {
        // The gRPC zone's address presented as a WT endpoint.
        assert_eq!(wt("https://192.168.3.7:50053"), rej(W, H));
        // The WT zone's address presented as a gRPC endpoint.
        assert_eq!(grpc("grpc://172.18.0.1:4440"), rej(G, H));
    }

    #[test]
    fn suffix_entry_never_matches_ip_host() {
        let a = MhEndpointAllowlist::new(
            parse_wt_hosts(".mh.example.com").unwrap(),
            parse_port_set("4440").unwrap(),
            parse_scheme_set("grpc").unwrap(),
            parse_cidr_set("192.168.0.0/16").unwrap(),
            parse_port_set("50053").unwrap(),
        );
        assert!(a.check("https://x.mh.example.com:4440", GOOD_GRPC).is_ok());
        assert_eq!(
            a.check("https://10.0.0.1:4440", GOOD_GRPC),
            rej(W, H),
            "a suffix entry never matches an IP literal"
        );
        // An IP-shaped suffix cannot even be configured.
        assert!(parse_wt_hosts(".1").is_err());
        assert!(parse_wt_hosts(".0.1").is_err());
    }

    #[test]
    fn rejection_display_never_echoes_allowlist() {
        let r = wt("https://172.18.0.2:4440").unwrap_err();
        assert_eq!(r.to_string(), "webtransport_endpoint: host not allowed");
        assert!(!r.to_string().contains("172.18.0.1"));
    }

    #[test]
    fn config_parsers_reject_wildcards_and_empties() {
        for raw in [
            "",
            " ",
            "a.example.com,",
            ",a.example.com",
            "*",
            ".",
            "A.example.com",
            "x.com.",
        ] {
            assert!(parse_wt_hosts(raw).is_err(), "hosts {raw:?}");
        }
        assert!(parse_wt_hosts("a.example.com, .b.example.com").is_ok());
        for raw in ["", "0", "65536", "-1", "080", "4440,", "x"] {
            assert!(parse_port_set(raw).is_err(), "ports {raw:?}");
        }
        assert!(parse_port_set("1,65535").is_ok());
        for raw in ["", "*", "GRPC", "grpc,", "1grpc", "gr pc"] {
            assert!(parse_scheme_set(raw).is_err(), "schemes {raw:?}");
        }
        assert!(parse_scheme_set("grpc,grpc+tls").is_ok());
        for raw in [
            "",
            "0.0.0.0/0",
            "::/0",
            "10.0.0.1/8",
            "10.0.0/8",
            "10.0.0.0",
            "10.0.0.0/33",
        ] {
            assert!(parse_cidr_set(raw).is_err(), "cidrs {raw:?}");
        }
        assert!(parse_cidr_set("10.0.0.0/8,fd00::/8").is_ok());
    }

    #[test]
    fn config_error_names_offending_entry() {
        let e = parse_cidr_set("10.0.0.1/8").unwrap_err();
        assert!(
            e.contains("'10.0.0.1/8'") && e.contains("10.0.0.0/8"),
            "{e}"
        );
    }
}
