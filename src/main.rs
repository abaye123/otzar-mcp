//! otzar-mcp: a local MCP server over the Otzar HaHochma library disk.
//!
//! Transport is stdio only. Logs go to stderr, never stdout (stdout carries the protocol).
//! Windows only: Otzar HaHochma exists only for Windows, and the code relies on drive letters,
//! Windows paths and Windows process flags.

#[cfg(not(windows))]
compile_error!("otzar-mcp supports Windows only (Otzar HaHochma has no macOS or Linux version)");

mod catalog;
mod config;
mod hebrew;
mod server;
mod words;

use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OTZAR_MCP_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let (config, config_path) = config::Config::load()?;
    tracing::info!(config = ?config_path, "starting otzar-mcp {}", env!("CARGO_PKG_VERSION"));

    let service = server::OtzarServer::new(server::State::new(config, config_path))
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serve error: {e:?}"))?;
    service.waiting().await?;
    Ok(())
}
