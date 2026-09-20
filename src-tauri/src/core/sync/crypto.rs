use std::io::{Read, Write};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

use super::{Result, SyncError};

const STREAM_MAGIC: &[u8; 8] = b"TRCENC01";
pub const STREAM_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_kib: 64 * 1024,
            iterations: 3,
            parallelism: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedMasterKey {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

pub fn derive_password_key(password: &str, salt: &[u8], params: &KdfParams) -> Result<[u8; 32]> {
    if password.is_empty() {
        return Err(SyncError::Crypto("同步密码不能为空".into()));
    }
    let p = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(32),
    )
    .map_err(|e| SyncError::Crypto(format!("KDF 参数无效: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, p);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| SyncError::Crypto(format!("密码派生失败: {e}")))?;
    Ok(key)
}

pub fn wrap_master_key(password_key: &[u8; 32], master_key: &[u8; 32]) -> Result<WrappedMasterKey> {
    let nonce = random_bytes::<24>();
    let cipher = XChaCha20Poly1305::new(password_key.into());
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: master_key,
                aad: b"tracer-master-key-v1",
            },
        )
        .map_err(|_| SyncError::Crypto("主密钥封装失败".into()))?;
    Ok(WrappedMasterKey {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

pub fn unwrap_master_key(password_key: &[u8; 32], wrapped: &WrappedMasterKey) -> Result<[u8; 32]> {
    if wrapped.nonce.len() != 24 {
        return Err(SyncError::Format("主密钥 nonce 长度错误".into()));
    }
    let cipher = XChaCha20Poly1305::new(password_key.into());
    let plain = cipher
        .decrypt(
            XNonce::from_slice(&wrapped.nonce),
            Payload {
                msg: &wrapped.ciphertext,
                aad: b"tracer-master-key-v1",
            },
        )
        .map_err(|_| SyncError::Crypto("同步密码错误或控制文件已损坏".into()))?;
    plain
        .try_into()
        .map_err(|_| SyncError::Format("主密钥长度错误".into()))
}

pub fn seal_bytes(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let nonce = random_bytes::<24>();
    let cipher = XChaCha20Poly1305::new(key.into());
    let data = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| SyncError::Crypto("加密失败".into()))?;
    Ok((nonce.to_vec(), data))
}

pub fn open_bytes(key: &[u8; 32], aad: &[u8], nonce: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    if nonce.len() != 24 {
        return Err(SyncError::Format("nonce 长度错误".into()));
    }
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: data, aad })
        .map_err(|_| SyncError::Crypto("认证失败：密码错误或数据已被篡改".into()))
}

fn stream_nonce(prefix: &[u8; 16], index: u64) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[..16].copy_from_slice(prefix);
    nonce[16..].copy_from_slice(&index.to_le_bytes());
    nonce
}
fn stream_aad(context: &[u8], index: u64, is_final: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(context.len() + 9);
    aad.extend_from_slice(context);
    aad.extend_from_slice(&index.to_le_bytes());
    aad.push(is_final as u8);
    aad
}

pub fn encrypt_stream<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    key: &[u8; 32],
    context: &[u8],
) -> Result<u64> {
    let prefix = random_bytes::<16>();
    writer.write_all(STREAM_MAGIC)?;
    writer.write_all(&(STREAM_CHUNK_SIZE as u32).to_le_bytes())?;
    writer.write_all(&prefix)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut index = 0u64;
    let mut total = 28u64;
    let mut current = vec![0u8; STREAM_CHUNK_SIZE];
    let mut current_len = read_chunk(&mut reader, &mut current)?;
    loop {
        let mut next = vec![0u8; STREAM_CHUNK_SIZE];
        let next_len = read_chunk(&mut reader, &mut next)?;
        let final_chunk = next_len == 0;
        let nonce = stream_nonce(&prefix, index);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &current[..current_len],
                    aad: &stream_aad(context, index, final_chunk),
                },
            )
            .map_err(|_| SyncError::Crypto("年度包加密失败".into()))?;
        writer.write_all(&(ciphertext.len() as u32).to_le_bytes())?;
        writer.write_all(&[final_chunk as u8])?;
        writer.write_all(&ciphertext)?;
        total += 5 + ciphertext.len() as u64;
        if final_chunk {
            break;
        }
        current = next;
        current_len = next_len;
        index += 1;
    }
    Ok(total)
}

pub fn decrypt_stream<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    key: &[u8; 32],
    context: &[u8],
) -> Result<u64> {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != STREAM_MAGIC {
        return Err(SyncError::Format("年度包文件标识不支持".into()));
    }
    let mut n4 = [0u8; 4];
    reader.read_exact(&mut n4)?;
    let chunk = u32::from_le_bytes(n4) as usize;
    if chunk == 0 || chunk > 16 * 1024 * 1024 {
        return Err(SyncError::Format("年度包分块大小无效".into()));
    }
    let mut prefix = [0u8; 16];
    reader.read_exact(&mut prefix)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut index = 0u64;
    let mut total = 0u64;
    loop {
        reader.read_exact(&mut n4)?;
        let len = u32::from_le_bytes(n4) as usize;
        if len < 16 || len > chunk + 16 {
            return Err(SyncError::Format("年度包分块长度无效".into()));
        }
        let mut flag = [0u8; 1];
        reader.read_exact(&mut flag)?;
        if flag[0] > 1 {
            return Err(SyncError::Format("年度包结束标记无效".into()));
        }
        let mut encrypted = vec![0u8; len];
        reader.read_exact(&mut encrypted)?;
        let nonce = stream_nonce(&prefix, index);
        let plain = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &encrypted,
                    aad: &stream_aad(context, index, flag[0] == 1),
                },
            )
            .map_err(|_| SyncError::Crypto("年度包认证失败或已损坏".into()))?;
        writer.write_all(&plain)?;
        total += plain.len() as u64;
        if flag[0] == 1 {
            let mut extra = [0u8; 1];
            if reader.read(&mut extra)? != 0 {
                return Err(SyncError::Format("年度包结束后存在多余数据".into()));
            }
            break;
        }
        index += 1;
    }
    Ok(total)
}

fn read_chunk<R: Read>(reader: &mut R, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut read = 0;
    while read < buffer.len() {
        let n = reader.read(&mut buffer[read..])?;
        if n == 0 {
            break;
        }
        read += n;
    }
    Ok(read)
}

#[cfg(windows)]
pub fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|e| SyncError::Crypto(format!("DPAPI 保护失败: {e}")))?;
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(result)
    }
}

#[cfg(windows)]
pub fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|e| SyncError::Crypto(format!("DPAPI 解封失败: {e}")))?;
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(result)
    }
}

#[cfg(not(windows))]
pub fn dpapi_protect(_: &[u8]) -> Result<Vec<u8>> {
    Err(SyncError::Crypto("DPAPI 仅支持 Windows".into()))
}
#[cfg(not(windows))]
pub fn dpapi_unprotect(_: &[u8]) -> Result<Vec<u8>> {
    Err(SyncError::Crypto("DPAPI 仅支持 Windows".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_roundtrip_tamper_truncation_and_context() {
        let key = random_bytes::<32>();
        let data = vec![7u8; STREAM_CHUNK_SIZE + 17];
        let mut encrypted = Vec::new();
        encrypt_stream(&data[..], &mut encrypted, &key, b"space/gen/2026").unwrap();
        let mut plain = Vec::new();
        decrypt_stream(&encrypted[..], &mut plain, &key, b"space/gen/2026").unwrap();
        assert_eq!(plain, data);
        assert!(decrypt_stream(&encrypted[..], Vec::new(), &key, b"space/gen/2025").is_err());
        let mut tampered = encrypted.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(decrypt_stream(&tampered[..], Vec::new(), &key, b"space/gen/2026").is_err());
        assert!(decrypt_stream(
            &encrypted[..encrypted.len() - 1],
            Vec::new(),
            &key,
            b"space/gen/2026"
        )
        .is_err());
    }
    #[test]
    fn random_nonce_changes_ciphertext() {
        let key = random_bytes::<32>();
        let mut a = Vec::new();
        let mut b = Vec::new();
        encrypt_stream(&b"same"[..], &mut a, &key, b"ctx").unwrap();
        encrypt_stream(&b"same"[..], &mut b, &key, b"ctx").unwrap();
        assert_ne!(a, b);
    }
    #[cfg(windows)]
    #[test]
    fn dpapi_roundtrip() {
        let protected = dpapi_protect(b"secret").unwrap();
        assert_ne!(protected, b"secret");
        assert_eq!(dpapi_unprotect(&protected).unwrap(), b"secret");
    }
}
