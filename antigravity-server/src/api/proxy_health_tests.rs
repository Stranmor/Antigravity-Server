#![allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "test assertions")]

use super::*;

#[test]
fn test_validate_proxy_url_socks5() {
    assert!(validate_proxy_url("socks5://1.2.3.4:1080").is_ok());
}

#[test]
fn test_validate_proxy_url_socks5h() {
    assert!(validate_proxy_url("socks5h://user:pass@host:1080").is_ok());
}

#[test]
fn test_validate_proxy_url_http() {
    assert!(validate_proxy_url("http://proxy:8080").is_ok());
}

#[test]
fn test_validate_proxy_url_https() {
    assert!(validate_proxy_url("https://proxy:8443").is_ok());
}

#[test]
fn test_validate_proxy_url_invalid_scheme() {
    let result = validate_proxy_url("ftp://proxy:21");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid proxy URL scheme"));
}

#[test]
fn test_validate_proxy_url_no_scheme() {
    assert!(validate_proxy_url("127.0.0.1:1080").is_err());
}

#[test]
fn test_validate_proxy_url_empty() {
    assert!(validate_proxy_url("").is_err());
}

#[test]
fn test_reject_private_ips() {
    assert!(validate_proxy_url("socks5://127.0.0.1:1080").is_err());
    assert!(validate_proxy_url("socks5://10.0.0.1:1080").is_err());
    assert!(validate_proxy_url("socks5://192.168.1.1:1080").is_err());
    assert!(validate_proxy_url("socks5://172.16.0.1:1080").is_err());
}

#[test]
fn test_reject_localhost() {
    assert!(validate_proxy_url("http://localhost:8080").is_err());
    assert!(validate_proxy_url("http://something.local:8080").is_err());
}

#[test]
fn test_reject_metadata_ip() {
    assert!(validate_proxy_url("http://169.254.169.254:80").is_err());
}

#[test]
fn test_reject_ipv6_loopback() {
    assert!(validate_proxy_url("socks5://[::1]:1080").is_err());
}

#[test]
fn test_reject_ipv6_mapped_ipv4_loopback() {
    assert!(validate_proxy_url("socks5://[::ffff:127.0.0.1]:1080").is_err());
}

#[test]
fn test_reject_ipv6_mapped_ipv4_private() {
    assert!(validate_proxy_url("socks5://[::ffff:10.0.0.1]:1080").is_err());
    assert!(validate_proxy_url("socks5://[::ffff:192.168.1.1]:1080").is_err());
    assert!(validate_proxy_url("socks5://[::ffff:169.254.169.254]:1080").is_err());
}

#[test]
fn test_reject_ipv6_unique_local() {
    assert!(validate_proxy_url("socks5://[fd00::1]:1080").is_err());
    assert!(validate_proxy_url("socks5://[fc00::1]:1080").is_err());
}

#[test]
fn test_reject_ipv6_link_local() {
    assert!(validate_proxy_url("socks5://[fe80::1]:1080").is_err());
}

#[test]
fn test_reject_unspecified_ipv4() {
    assert!(validate_proxy_url("socks5://0.0.0.0:1080").is_err());
}

#[test]
fn test_reject_ipv4_compatible_ipv6_loopback() {
    assert!(validate_proxy_url("socks5://[::7f00:1]:1080").is_err());
}

#[test]
fn test_reject_ipv4_compatible_ipv6_private() {
    assert!(validate_proxy_url("socks5://[::a00:1]:1080").is_err());
}

#[test]
fn test_reject_no_host() {
    assert!(validate_proxy_url("socks5://").is_err());
}

#[test]
fn test_reject_multicast_ipv4() {
    assert!(validate_proxy_url("socks5://224.0.0.1:1080").is_err());
    assert!(validate_proxy_url("socks5://239.255.255.255:1080").is_err());
}

#[test]
fn test_reject_broadcast_ipv4() {
    assert!(validate_proxy_url("socks5://255.255.255.255:1080").is_err());
}

#[test]
fn test_reject_multicast_ipv6() {
    assert!(validate_proxy_url("socks5://[ff02::1]:1080").is_err());
}

#[test]
fn test_accept_domain_proxy_url() {
    assert!(validate_proxy_url("socks5h://proxy.example.com:1080").is_ok());
    assert!(validate_proxy_url("socks5h://user:pass@proxy.example.com:1080").is_ok());
    assert!(validate_proxy_url("http://proxy.example.com:8080").is_ok());
}
