use anyhow::Result;
use listenfd::ListenFd;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use tokio::signal;
use tracing::info;

pub async fn create_listener(
    port: u16,
    proxy_config: &antigravity_types::models::ProxyConfig,
) -> Result<tokio::net::TcpListener> {
    let mut listenfd = ListenFd::from_env();

    if let Some(listener) = listenfd.take_tcp_listener(0)? {
        info!("🔌 Using systemd socket activation (fd=3)");
        listener.set_nonblocking(true)?;
        return Ok(tokio::net::TcpListener::from_std(listener)?);
    }

    let bind_addr = proxy_config.get_bind_address();
    let ip: std::net::IpAddr = bind_addr
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address '{}': {}", bind_addr, e))?;
    let addr = SocketAddr::new(ip, port);
    let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(4096)?;

    info!("🔌 Binding with SO_REUSEPORT to {} (zero-downtime capable)", addr);

    Ok(tokio::net::TcpListener::from_std(socket.into())?)
}

#[allow(
    clippy::expect_used,
    reason = "Signal handlers are critical infrastructure, panic is appropriate on failure"
)]
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("🛑 Received Ctrl+C, initiating graceful shutdown..."),
        () = terminate => info!("🛑 Received SIGTERM, initiating graceful shutdown..."),
    }

    info!("⏳ Graceful shutdown initiated, draining in-flight requests...");
}
