use std::{fs::File, io::Read, path::Path};

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::core::repo::{AppMetadataRecord, CategoryRecord, DeviceRecord, SegmentRecord};

use super::{
    crypto::{self, KdfParams, WrappedMasterKey},
    Result, SyncError,
};

pub const MANIFEST_FORMAT_VERSION: u32 = 1;
pub const PACKAGE_FORMAT_VERSION: u32 = 1;
const MANIFEST_MAGIC: &str = "TRACER-MANIFEST";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageDescriptor {
    pub year: i32,
    pub revision: String,
    pub sha256: String,
    pub encrypted_bytes: u64,
    pub session_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPayload {
    pub sync_space_id: String,
    pub generation_id: String,
    pub packages: Vec<PackageDescriptor>,
    pub devices: Vec<DeviceRecord>,
    pub apps: Vec<AppMetadataRecord>,
    pub categories: Vec<CategoryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestHeader {
    pub magic: String,
    pub version: u32,
    pub kdf: KdfParams,
    pub salt: String,
    pub wrapped_master_key: WrappedMasterKey,
    pub payload_nonce: String,
    pub payload_ciphertext: String,
}

pub struct OpenManifest {
    pub payload: ManifestPayload,
    pub master_key: [u8; 32],
}

pub fn new_space(password: &str) -> Result<(Vec<u8>, ManifestPayload, [u8; 32])> {
    let payload = ManifestPayload {
        sync_space_id: uuid::Uuid::new_v4().to_string(),
        generation_id: uuid::Uuid::new_v4().to_string(),
        packages: Vec::new(),
        devices: Vec::new(),
        apps: Vec::new(),
        categories: Vec::new(),
    };
    let master = crypto::random_bytes::<32>();
    let bytes = encode_manifest(password, &master, &payload)?;
    Ok((bytes, payload, master))
}

pub fn encode_manifest(
    password: &str,
    master_key: &[u8; 32],
    payload: &ManifestPayload,
) -> Result<Vec<u8>> {
    let kdf = KdfParams::default();
    let salt = crypto::random_bytes::<16>();
    let password_key = crypto::derive_password_key(password, &salt, &kdf)?;
    let wrapped = crypto::wrap_master_key(&password_key, master_key)?;
    encode_manifest_with_wrapped(&kdf, &salt, &wrapped, master_key, payload)
}

pub fn encode_manifest_with_wrapped(
    kdf: &KdfParams,
    salt: &[u8],
    wrapped: &WrappedMasterKey,
    master_key: &[u8; 32],
    payload: &ManifestPayload,
) -> Result<Vec<u8>> {
    let plain = serde_json::to_vec(payload)
        .map_err(|e| SyncError::Format(format!("控制内容编码失败: {e}")))?;
    let (nonce, ciphertext) =
        crypto::seal_bytes(master_key, b"tracer-manifest-payload-v1", &plain)?;
    let header = ManifestHeader {
        magic: MANIFEST_MAGIC.into(),
        version: MANIFEST_FORMAT_VERSION,
        kdf: kdf.clone(),
        salt: STANDARD.encode(salt),
        wrapped_master_key: wrapped.clone(),
        payload_nonce: STANDARD.encode(nonce),
        payload_ciphertext: STANDARD.encode(ciphertext),
    };
    serde_json::to_vec(&header).map_err(|e| SyncError::Format(format!("控制文件编码失败: {e}")))
}

pub fn open_manifest(bytes: &[u8], password: &str) -> Result<OpenManifest> {
    let header: ManifestHeader = serde_json::from_slice(bytes)
        .map_err(|e| SyncError::Format(format!("控制文件格式错误: {e}")))?;
    validate_header(&header)?;
    let salt = STANDARD
        .decode(&header.salt)
        .map_err(|_| SyncError::Format("控制文件 salt 无效".into()))?;
    let password_key = crypto::derive_password_key(password, &salt, &header.kdf)?;
    let master = crypto::unwrap_master_key(&password_key, &header.wrapped_master_key)?;
    open_manifest_with_master(bytes, &master).map(|payload| OpenManifest {
        payload,
        master_key: master,
    })
}

pub fn open_manifest_with_master(bytes: &[u8], master: &[u8; 32]) -> Result<ManifestPayload> {
    let header: ManifestHeader = serde_json::from_slice(bytes)
        .map_err(|e| SyncError::Format(format!("控制文件格式错误: {e}")))?;
    validate_header(&header)?;
    let nonce = STANDARD
        .decode(&header.payload_nonce)
        .map_err(|_| SyncError::Format("控制文件 nonce 无效".into()))?;
    let data = STANDARD
        .decode(&header.payload_ciphertext)
        .map_err(|_| SyncError::Format("控制内容编码无效".into()))?;
    let plain = crypto::open_bytes(master, b"tracer-manifest-payload-v1", &nonce, &data)?;
    serde_json::from_slice(&plain).map_err(|e| SyncError::Format(format!("控制内容格式错误: {e}")))
}

pub fn replace_manifest_payload(
    bytes: &[u8],
    master: &[u8; 32],
    payload: &ManifestPayload,
) -> Result<Vec<u8>> {
    let mut header: ManifestHeader = serde_json::from_slice(bytes)
        .map_err(|e| SyncError::Format(format!("控制文件格式错误: {e}")))?;
    validate_header(&header)?;
    let plain = serde_json::to_vec(payload)
        .map_err(|e| SyncError::Format(format!("控制内容编码失败: {e}")))?;
    let (nonce, ciphertext) = crypto::seal_bytes(master, b"tracer-manifest-payload-v1", &plain)?;
    header.payload_nonce = STANDARD.encode(nonce);
    header.payload_ciphertext = STANDARD.encode(ciphertext);
    serde_json::to_vec(&header).map_err(|e| SyncError::Format(format!("控制文件编码失败: {e}")))
}

pub fn change_password(
    bytes: &[u8],
    old_password: Option<&str>,
    cached_master: Option<[u8; 32]>,
    new_password: &str,
) -> Result<Vec<u8>> {
    let master = match cached_master {
        Some(v) => v,
        None => {
            open_manifest(
                bytes,
                old_password.ok_or_else(|| SyncError::Crypto("需要旧同步密码".into()))?,
            )?
            .master_key
        }
    };
    let payload = open_manifest_with_master(bytes, &master)?;
    encode_manifest(new_password, &master, &payload)
}

fn validate_header(header: &ManifestHeader) -> Result<()> {
    if header.magic != MANIFEST_MAGIC {
        return Err(SyncError::Format("不是 Tracer 控制文件".into()));
    }
    if header.version != MANIFEST_FORMAT_VERSION {
        return Err(SyncError::Format(format!(
            "不支持的控制文件版本: {}",
            header.version
        )));
    }
    Ok(())
}

fn package_context(space: &str, generation: &str, year: i32) -> Vec<u8> {
    format!("tracer-package-v{PACKAGE_FORMAT_VERSION}/{space}/{generation}/{year}").into_bytes()
}

pub fn build_year_package(
    path: &Path,
    space: &str,
    generation: &str,
    year: i32,
    revision: &str,
    segments: &[SegmentRecord],
    master: &[u8; 32],
) -> Result<PackageDescriptor> {
    let temp = TempDir::new()?;
    let sqlite_path = temp.path().join("year.sqlite");
    let compressed_path = temp.path().join("year.zst");
    {
        let mut conn = Connection::open(&sqlite_path)?;
        conn.execute_batch("PRAGMA journal_mode=OFF;PRAGMA synchronous=OFF;CREATE TABLE package_meta(format_version INTEGER NOT NULL,sync_space_id TEXT NOT NULL,generation_id TEXT NOT NULL,year INTEGER NOT NULL,revision TEXT NOT NULL);CREATE TABLE app_refs(origin_device_id TEXT NOT NULL,origin_app_id INTEGER NOT NULL,process_name TEXT NOT NULL,PRIMARY KEY(origin_device_id,origin_app_id)) WITHOUT ROWID;CREATE TABLE sessions(origin_device_id TEXT NOT NULL,segment_sequence INTEGER NOT NULL,origin_app_id INTEGER NOT NULL,start_utc INTEGER NOT NULL,end_utc INTEGER NOT NULL,source_local_date INTEGER NOT NULL,utc_offset_minutes INTEGER NOT NULL,PRIMARY KEY(origin_device_id,segment_sequence)) WITHOUT ROWID;")?;
        conn.execute(
            "INSERT INTO package_meta VALUES(?1,?2,?3,?4,?5)",
            params![PACKAGE_FORMAT_VERSION, space, generation, year, revision],
        )?;
        let tx = conn.transaction()?;
        for s in segments {
            tx.execute(
                "INSERT OR IGNORE INTO app_refs VALUES(?1,?2,?3)",
                params![s.origin_device_id, s.origin_app_id, s.process_name],
            )?;
            tx.execute(
                "INSERT INTO sessions VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    s.origin_device_id,
                    s.segment_sequence,
                    s.origin_app_id,
                    s.start_utc,
                    s.end_utc,
                    s.source_local_date,
                    s.utc_offset_minutes
                ],
            )?;
        }
        tx.commit()?;
    }
    zstd::stream::copy_encode(
        File::open(&sqlite_path)?,
        File::create(&compressed_path)?,
        6,
    )
    .map_err(SyncError::Io)?;
    let encrypted_bytes = crypto::encrypt_stream(
        File::open(&compressed_path)?,
        File::create(path)?,
        master,
        &package_context(space, generation, year),
    )?;
    let sha256 = file_sha256(path)?;
    Ok(PackageDescriptor {
        year,
        revision: revision.into(),
        sha256,
        encrypted_bytes,
        session_count: segments.len() as u64,
    })
}

pub fn open_year_package(
    path: &Path,
    expected: &PackageDescriptor,
    space: &str,
    generation: &str,
    master: &[u8; 32],
) -> Result<Vec<SegmentRecord>> {
    if file_sha256(path)? != expected.sha256 {
        return Err(SyncError::Format("年度包摘要不匹配".into()));
    }
    let temp = TempDir::new()?;
    let compressed = temp.path().join("year.zst");
    let sqlite = temp.path().join("year.sqlite");
    crypto::decrypt_stream(
        File::open(path)?,
        File::create(&compressed)?,
        master,
        &package_context(space, generation, expected.year),
    )?;
    zstd::stream::copy_decode(File::open(&compressed)?, File::create(&sqlite)?)
        .map_err(SyncError::Io)?;
    let conn = Connection::open_with_flags(&sqlite, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let meta: (u32, String, String, i32, String) = conn.query_row(
        "SELECT format_version,sync_space_id,generation_id,year,revision FROM package_meta",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    if meta
        != (
            PACKAGE_FORMAT_VERSION,
            space.into(),
            generation.into(),
            expected.year,
            expected.revision.clone(),
        )
    {
        return Err(SyncError::Format("年度包元数据与控制文件不一致".into()));
    }
    let mut stmt=conn.prepare("SELECT s.origin_device_id,s.segment_sequence,s.origin_app_id,a.process_name,s.start_utc,s.end_utc,s.source_local_date,s.utc_offset_minutes FROM sessions s JOIN app_refs a USING(origin_device_id,origin_app_id) ORDER BY s.origin_device_id,s.segment_sequence")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SegmentRecord {
                origin_device_id: r.get(0)?,
                segment_sequence: r.get(1)?,
                origin_app_id: r.get(2)?,
                process_name: r.get(3)?,
                start_utc: r.get(4)?,
                end_utc: r.get(5)?,
                source_local_date: r.get(6)?,
                utc_offset_minutes: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() as u64 != expected.session_count {
        return Err(SyncError::Format("年度包会话数量不匹配".into()));
    }
    Ok(rows)
}

pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sync::crypto;
    #[test]
    fn manifest_password_and_change() {
        let (bytes, payload, key) = new_space("old").unwrap();
        assert_eq!(
            open_manifest(&bytes, "old").unwrap().payload.sync_space_id,
            payload.sync_space_id
        );
        assert!(open_manifest(&bytes, "bad").is_err());
        let changed = change_password(&bytes, None, Some(key), "new").unwrap();
        assert!(open_manifest(&changed, "old").is_err());
        assert_eq!(
            open_manifest(&changed, "new")
                .unwrap()
                .payload
                .generation_id,
            payload.generation_id
        );
    }
    #[test]
    fn package_roundtrip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("2026.tracer-sync");
        let key = crypto::random_bytes::<32>();
        let rows = vec![SegmentRecord {
            origin_device_id: "d".into(),
            segment_sequence: 1,
            origin_app_id: 2,
            process_name: "x.exe".into(),
            start_utc: 1,
            end_utc: 2,
            source_local_date: 20260101,
            utc_offset_minutes: 480,
        }];
        let descriptor = build_year_package(&path, "s", "g", 2026, "r", &rows, &key).unwrap();
        assert_eq!(
            open_year_package(&path, &descriptor, "s", "g", &key).unwrap(),
            rows
        );
    }
}
