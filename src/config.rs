//! Runtime configuration (`otzar-mcp.toml`) and Otzar HaHochma disk detection.

use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Catalog copy that the official app itself uses (see `db` in the disk's `config.json`).
pub const DEFAULT_CATALOG: &str = r"C:\OtzarApp\OtzarLocal\data.db";
/// The official Electron app. It accepts `BOOK=<id>` on the command line.
pub const DEFAULT_OTZAR_EXE: &str = r"C:\OtzarApp\OtzarLocal\launcher\bin\x64\app\otzar.exe";
/// Config file name, looked up next to the executable unless `OTZAR_MCP_CONFIG` is set.
pub const CONFIG_FILE_NAME: &str = "otzar-mcp.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Root of the Otzar HaHochma disk (e.g. `D:\`). Auto-detected when omitted.
    pub disk_root: Option<PathBuf>,
    /// Path to the catalog `data.db`. Defaults to the app's copy, then `<disk>\data\data.db`.
    pub catalog_db: Option<PathBuf>,
    /// Path to the official `otzar.exe`.
    pub otzar_exe: Option<PathBuf>,
    pub subscription: Subscription,
    pub limits: Limits,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Subscription {
    /// Package/permission codes the user's subscription includes (the `name` column of the
    /// `packages` and `permissions` tables, e.g. "KOOK", "CHABAD").
    /// `None` means unknown: restricted books are reported with status `unknown`.
    pub packages: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Hard cap for any `limit` argument.
    pub max_results: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_results: 100 }
    }
}

impl Config {
    /// Loads the config file if present; a missing file yields defaults.
    pub fn load() -> anyhow::Result<(Self, Option<PathBuf>)> {
        let path = std::env::var_os("OTZAR_MCP_CONFIG")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|exe| exe.parent().map(|dir| dir.join(CONFIG_FILE_NAME)))
            });
        match path {
            Some(path) if path.is_file() => {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                let config: Config =
                    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
                Ok((config, Some(path)))
            }
            _ => Ok((Self::default(), None)),
        }
    }

    pub fn otzar_exe(&self) -> PathBuf {
        self.otzar_exe
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_OTZAR_EXE))
    }

    /// Catalog database: explicit path, else the app's copy, else the copy on the disk.
    pub fn catalog_db(&self, disk: Option<&Path>) -> Option<PathBuf> {
        if let Some(path) = &self.catalog_db {
            return Some(path.clone());
        }
        let app_copy = PathBuf::from(DEFAULT_CATALOG);
        if app_copy.is_file() {
            return Some(app_copy);
        }
        disk.map(|root| root.join("data").join("data.db"))
            .filter(|path| path.is_file())
    }

    /// Locates the disk. Called per request so that plugging the disk in works without a restart.
    pub fn find_disk(&self) -> Option<PathBuf> {
        if let Some(root) = &self.disk_root {
            return is_otzar_disk(root).then(|| root.clone());
        }
        // Detect by content, not by letter: Windows may assign a different letter next time.
        (b'C'..=b'Z')
            .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
            .find(|root| is_otzar_disk(root))
    }
}

pub fn is_otzar_disk(root: &Path) -> bool {
    root.join("searcher").join("fs.db").is_file()
        && root.join("books").is_dir()
        && root.join("data").is_dir()
}

/// Reads a small text file from the disk (version markers etc.), trimmed.
pub fn read_marker(root: &Path, relative: &str) -> Option<String> {
    std::fs::read_to_string(root.join(relative))
        .ok()
        .map(|s| s.trim().to_string())
}
