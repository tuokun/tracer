use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::UnboundedSender, oneshot, Mutex};

use crate::core::{event::Event, repo};

use super::{
    merge, package,
    webdav::{ConcurrencyMode, WebDavClient},
    Result, SyncError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSyncConfig {
    pub endpoint: String,
    pub directory: Option<String>,
    pub protected_credentials: String,
    pub protected_master_key: String,
    pub concurrency: ConcurrencyMode,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

impl StoredSyncConfig {
    pub fn new(
        endpoint: String,
        directory: Option<String>,
        username: String,
        password: String,
        master: &[u8; 32],
        concurrency: ConcurrencyMode,
    ) -> Result<Self> {
        let credentials = serde_json::to_vec(&Credentials { username, password })
            .map_err(|e| SyncError::Format(e.to_string()))?;
        Ok(Self {
            endpoint,
            directory,
            protected_credentials: STANDARD.encode(super::crypto::dpapi_protect(&credentials)?),
            protected_master_key: STANDARD.encode(super::crypto::dpapi_protect(master)?),
            concurrency,
        })
    }
    pub fn client_and_master(&self) -> Result<(WebDavClient, [u8; 32])> {
        let protected = STANDARD
            .decode(&self.protected_credentials)
            .map_err(|_| SyncError::Format("本机同步凭据格式错误".into()))?;
        let plain = super::crypto::dpapi_unprotect(&protected)?;
        let credentials: Credentials = serde_json::from_slice(&plain)
            .map_err(|_| SyncError::Format("本机同步凭据格式错误".into()))?;
        let key = STANDARD
            .decode(&self.protected_master_key)
            .map_err(|_| SyncError::Format("本机主密钥格式错误".into()))?;
        let key = super::crypto::dpapi_unprotect(&key)?;
        let master: [u8; 32] = key
            .try_into()
            .map_err(|_| SyncError::Format("本机主密钥长度错误".into()))?;
        Ok((
            WebDavClient::new(
                &self.endpoint,
                credentials.username,
                credentials.password,
                self.directory.as_deref(),
            )?,
            master,
        ))
    }
    pub fn credentials(&self) -> Result<(String, String)> {
        let protected = STANDARD
            .decode(&self.protected_credentials)
            .map_err(|_| SyncError::Format("本机同步凭据格式错误".into()))?;
        let plain = super::crypto::dpapi_unprotect(&protected)?;
        let value: Credentials = serde_json::from_slice(&plain)
            .map_err(|_| SyncError::Format("本机同步凭据格式错误".into()))?;
        Ok((value.username, value.password))
    }
}

pub fn save_stored_config(conn: &Connection, config: &StoredSyncConfig) -> Result<()> {
    let value = serde_json::to_string(config).map_err(|e| SyncError::Format(e.to_string()))?;
    repo::set_config(conn, "sync_config", &value)?;
    Ok(())
}
pub fn load_stored_config(conn: &Connection) -> Result<Option<StoredSyncConfig>> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key='sync_config'",
            [],
            |r| r.get(0),
        )
        .ok();
    value
        .map(|v| {
            serde_json::from_str(&v).map_err(|_| SyncError::Format("本机同步配置已损坏".into()))
        })
        .transpose()
}
pub fn disconnect(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM config WHERE key='sync_config'", [])?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    Idle,
    Checkpoint,
    Download,
    Decrypt,
    Merge,
    Upload,
    Complete,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub running: bool,
    pub phase: SyncPhase,
    pub year: Option<i32>,
    pub message: Option<String>,
}
impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            running: false,
            phase: SyncPhase::Idle,
            year: None,
            message: None,
        }
    }
}

pub struct SyncManager {
    status: Arc<Mutex<SyncStatus>>,
    cancel: Arc<AtomicBool>,
    owner: UnboundedSender<Event>,
    db_path: PathBuf,
}
impl SyncManager {
    pub fn new(owner: UnboundedSender<Event>, db_path: PathBuf) -> Self {
        Self {
            status: Arc::new(Mutex::new(SyncStatus::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            owner,
            db_path,
        }
    }
    pub async fn status(&self) -> SyncStatus {
        self.status.lock().await.clone()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub async fn is_running(&self) -> bool {
        self.status.lock().await.running
    }
    pub async fn checkpoint(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.owner
            .send(Event::SyncCheckpoint { ack: tx })
            .map_err(|_| SyncError::Cancelled)?;
        rx.await
            .map_err(|_| SyncError::Cancelled)?
            .map_err(SyncError::Database)
    }
    pub async fn replace_year(
        &self,
        year: i32,
        records: Vec<repo::SegmentRecord>,
    ) -> Result<usize> {
        let (tx, rx) = oneshot::channel();
        self.owner
            .send(Event::SyncReplaceYear {
                year,
                records,
                ack: tx,
            })
            .map_err(|_| SyncError::Cancelled)?;
        rx.await
            .map_err(|_| SyncError::Cancelled)?
            .map_err(SyncError::Database)
    }
    pub async fn run(
        &self,
        client: WebDavClient,
        master: [u8; 32],
        year: i32,
        mode: ConcurrencyMode,
    ) -> Result<SyncRunResult> {
        {
            let mut s = self.status.lock().await;
            if s.running {
                return Err(SyncError::Format("已有同步任务正在运行".into()));
            }
            *s = SyncStatus {
                running: true,
                phase: SyncPhase::Checkpoint,
                year: Some(year),
                message: None,
            };
        }
        self.cancel.store(false, Ordering::SeqCst);
        let started = Instant::now();
        let mut attempts = 0;
        let result = loop {
            let lock = if mode == ConcurrencyMode::Lock {
                match client.lock("manifest.tracer").await {
                    Ok(v) => v,
                    Err(e) => break Err(e),
                }
            } else {
                None
            };
            let attempt = self.run_inner(&client, &master, year, mode).await;
            if let Some(token) = lock {
                let _ = client.unlock("manifest.tracer", &token).await;
            }
            match attempt {
                Err(SyncError::Conflict) if attempts < 2 => {
                    attempts += 1;
                    continue;
                }
                other => break other,
            }
        };
        let mut state = self.status.lock().await;
        state.running = false;
        match &result {
            Ok(_) => {
                state.phase = SyncPhase::Complete;
                state.message = None
            }
            Err(SyncError::Cancelled) => {
                state.phase = SyncPhase::Cancelled;
                state.message = Some("同步已取消".into())
            }
            Err(e) => {
                state.phase = SyncPhase::Failed;
                state.message = Some(e.to_string())
            }
        }
        drop(state);
        result.map(|mut v| {
            v.duration_ms = started.elapsed().as_millis() as u64;
            v
        })
    }
    async fn run_inner(
        &self,
        client: &WebDavClient,
        master: &[u8; 32],
        year: i32,
        mode: ConcurrencyMode,
    ) -> Result<SyncRunResult> {
        self.check_cancel()?;
        let (ack_tx, ack_rx) = oneshot::channel();
        self.owner
            .send(Event::SyncCheckpoint { ack: ack_tx })
            .map_err(|_| SyncError::Cancelled)?;
        ack_rx
            .await
            .map_err(|_| SyncError::Cancelled)?
            .map_err(SyncError::Database)?;
        self.set_phase(SyncPhase::Download).await;
        let remote_manifest = client
            .get("manifest.tracer")
            .await?
            .ok_or_else(|| SyncError::Format("远端控制文件不存在".into()))?;
        let mut manifest = package::open_manifest_with_master(&remote_manifest.bytes, master)?;
        let package_name = format!("{year}.tracer-sync");
        let remote = client.get(&package_name).await?;
        let downloaded_bytes = remote.as_ref().map_or(0, |v| v.bytes.len() as u64);
        let mut remote_segments = Vec::new();
        if let Some(remote_file) = &remote {
            if let Some(descriptor) = manifest.packages.iter().find(|p| p.year == year) {
                self.set_phase(SyncPhase::Decrypt).await;
                let temp = tempfile::NamedTempFile::new()?;
                std::fs::write(temp.path(), &remote_file.bytes)?;
                remote_segments = package::open_year_package(
                    temp.path(),
                    descriptor,
                    &manifest.sync_space_id,
                    &manifest.generation_id,
                    master,
                )?;
            }
        }
        self.check_cancel()?;
        self.set_phase(SyncPhase::Merge).await;
        let mut imported = 0usize;
        for chunk in remote_segments.chunks(500) {
            let (tx, rx) = oneshot::channel();
            self.owner
                .send(Event::SyncImport {
                    records: chunk.to_vec(),
                    ack: tx,
                })
                .map_err(|_| SyncError::Cancelled)?;
            imported += rx
                .await
                .map_err(|_| SyncError::Cancelled)?
                .map_err(SyncError::Database)?;
            tokio::task::yield_now().await;
            self.check_cancel()?;
        }
        let conn = crate::core::db::open(&self.db_path)?;
        merge::apply_metadata(
            &conn,
            &manifest.devices,
            &manifest.apps,
            &manifest.categories,
        )?;
        let local_id = crate::core::db::local_device_id(&conn)?;
        let local_segments = repo::export_segments(&conn, year)?;
        let merged = merge::merge_segments(&local_segments, &remote_segments)?;
        let local_devices = repo::list_devices(&conn)?;
        let mut devices = merge::merge_devices(&manifest.devices, &local_devices)?;
        devices.retain(|v| v.device_id != local_id);
        devices.extend(
            local_devices
                .into_iter()
                .filter(|v| v.device_id == local_id),
        );
        let local_apps = repo::export_app_metadata(&conn)?;
        let mut apps = merge::merge_apps(&manifest.apps, &local_apps)?;
        apps.retain(|v| v.origin_device_id != local_id);
        apps.extend(
            local_apps
                .into_iter()
                .filter(|v| v.origin_device_id == local_id),
        );
        let categories =
            merge::merge_categories(&manifest.categories, &repo::export_categories(&conn)?);
        manifest.devices = devices;
        manifest.apps = apps;
        manifest.categories = categories;
        self.set_phase(SyncPhase::Upload).await;
        let temp = tempfile::NamedTempFile::new()?;
        let revision = uuid::Uuid::new_v4().to_string();
        let descriptor = package::build_year_package(
            temp.path(),
            &manifest.sync_space_id,
            &manifest.generation_id,
            year,
            &revision,
            &merged,
            master,
        )?;
        let bytes = std::fs::read(temp.path())?;
        let uploaded_bytes = bytes.len() as u64;
        self.check_cancel()?;
        if mode == ConcurrencyMode::None {
            let latest = client
                .get("manifest.tracer")
                .await?
                .ok_or(SyncError::Conflict)?;
            if latest.bytes != remote_manifest.bytes {
                return Err(SyncError::Conflict);
            }
            let latest_package = client.get(&package_name).await?;
            let unchanged = match (&remote, &latest_package) {
                (None, None) => true,
                (Some(a), Some(b)) => a.bytes == b.bytes,
                _ => false,
            };
            if !unchanged {
                return Err(SyncError::Conflict);
            }
        }
        let expected = if mode == ConcurrencyMode::Etag {
            remote.as_ref().and_then(|v| v.etag.as_deref())
        } else {
            None
        };
        client
            .put(&package_name, bytes, expected, remote.is_none())
            .await?;
        manifest.packages.retain(|p| p.year != year);
        manifest.packages.push(descriptor);
        manifest.packages.sort_by_key(|p| p.year);
        let manifest_bytes =
            package::replace_manifest_payload(&remote_manifest.bytes, master, &manifest)?;
        client
            .put(
                "manifest.tracer",
                manifest_bytes,
                if mode == ConcurrencyMode::Etag {
                    remote_manifest.etag.as_deref()
                } else {
                    None
                },
                false,
            )
            .await?;
        Ok(SyncRunResult {
            imported_segments: imported,
            downloaded_bytes,
            uploaded_bytes,
            duration_ms: 0,
        })
    }
    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(SyncError::Cancelled)
        } else {
            Ok(())
        }
    }
    async fn set_phase(&self, phase: SyncPhase) {
        self.status.lock().await.phase = phase;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncRunResult {
    pub imported_segments: usize,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub duration_ms: u64,
}
