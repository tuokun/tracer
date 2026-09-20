use reqwest::{
    header::{ETAG, IF_MATCH, IF_NONE_MATCH},
    Client, Method, StatusCode, Url,
};
use serde::{Deserialize, Serialize};

use super::{Result, SyncError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyMode {
    Etag,
    Lock,
    None,
}
#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub bytes: Vec<u8>,
    pub etag: Option<String>,
}

#[derive(Clone)]
pub struct WebDavClient {
    client: Client,
    base: Url,
    username: String,
    password: String,
}

impl WebDavClient {
    pub fn new(
        endpoint: &str,
        username: String,
        password: String,
        directory: Option<&str>,
    ) -> Result<Self> {
        let mut base = Url::parse(endpoint)
            .map_err(|e| SyncError::Network(format!("WebDAV 地址无效: {e}")))?;
        if base.scheme() != "https" && base.scheme() != "http" {
            return Err(SyncError::Network(
                "WebDAV 地址必须使用 http:// 或 https://".into(),
            ));
        }
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let folder = directory
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("tracer-sync");
        if folder.split('/').any(|v| v == ".." || v == ".") {
            return Err(SyncError::Network("同步目录不能包含相对路径片段".into()));
        }
        base = base
            .join(&format!("{}/", folder.trim_matches('/')))
            .map_err(|e| SyncError::Network(format!("同步目录无效: {e}")))?;
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| SyncError::Network(format!("WebDAV 客户端初始化失败: {e}")))?;
        Ok(Self {
            client,
            base,
            username,
            password,
        })
    }
    pub fn is_insecure(&self) -> bool {
        self.base.scheme() == "http"
    }
    fn url(&self, name: &str) -> Result<Url> {
        if name.contains('/') || name == ".." {
            return Err(SyncError::Network("远端文件名无效".into()));
        }
        self.base
            .join(name)
            .map_err(|e| SyncError::Network(format!("远端路径无效: {e}")))
    }
    fn auth(&self, b: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        b.basic_auth(&self.username, Some(&self.password))
    }

    pub async fn ensure_directory(&self) -> Result<()> {
        let method = Method::from_bytes(b"MKCOL").unwrap();
        let response = self
            .auth(self.client.request(method, self.base.clone()))
            .send()
            .await
            .map_err(net)?;
        if response.status().is_success()
            || matches!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED | StatusCode::CONFLICT
            )
        {
            Ok(())
        } else {
            Err(status_error("创建同步目录", response.status()))
        }
    }
    pub async fn get(&self, name: &str) -> Result<Option<RemoteFile>> {
        let response = self
            .auth(self.client.get(self.url(name)?))
            .send()
            .await
            .map_err(net)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(status_error("下载", response.status()));
        }
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = response.bytes().await.map_err(net)?.to_vec();
        Ok(Some(RemoteFile { bytes, etag }))
    }
    pub async fn put(
        &self,
        name: &str,
        data: Vec<u8>,
        expected_etag: Option<&str>,
        create_only: bool,
    ) -> Result<Option<String>> {
        let mut request = self.auth(self.client.put(self.url(name)?)).body(data);
        if let Some(etag) = expected_etag {
            request = request.header(IF_MATCH, etag);
        } else if create_only {
            request = request.header(IF_NONE_MATCH, "*");
        }
        let response = request.send().await.map_err(net)?;
        if matches!(
            response.status(),
            StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT
        ) {
            return Err(SyncError::Conflict);
        }
        if !response.status().is_success() {
            return Err(status_error("上传", response.status()));
        }
        Ok(response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned))
    }
    pub async fn delete(&self, name: &str) -> Result<()> {
        let response = self
            .auth(self.client.delete(self.url(name)?))
            .send()
            .await
            .map_err(net)?;
        if response.status() == StatusCode::NOT_FOUND || response.status().is_success() {
            Ok(())
        } else {
            Err(status_error("删除", response.status()))
        }
    }
    pub async fn list_names(&self) -> Result<Vec<String>> {
        let method = Method::from_bytes(b"PROPFIND").unwrap();
        let response = self
            .auth(
                self.client
                    .request(method, self.base.clone())
                    .header("Depth", "1"),
            )
            .send()
            .await
            .map_err(net)?;
        if !response.status().is_success() {
            return Err(status_error("列出目录", response.status()));
        }
        let xml = response.text().await.map_err(net)?;
        let mut names = Vec::new();
        for part in xml.split("</").filter(|v| v.contains("href>")) {
            if let Some(value) = part.split_once("href>").map(|v| v.1) {
                let decoded = value.replace("%20", " ");
                if let Some(name) = decoded.trim_end_matches('/').rsplit('/').next() {
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        Ok(names)
    }
    pub async fn detect_concurrency(&self) -> Result<ConcurrencyMode> {
        let probe = format!(".tracer-probe-{}", uuid::Uuid::new_v4());
        self.put(&probe, vec![1], None, true).await?;
        let etag = self.get(&probe).await?.and_then(|v| v.etag);
        let mode = if let Some(etag) = etag {
            match self.put(&probe, vec![2], Some(&etag), false).await {
                Ok(_) => match self.put(&probe, vec![3], Some(&etag), false).await {
                    Err(SyncError::Conflict) => ConcurrencyMode::Etag,
                    _ => self.try_lock(&probe).await,
                },
                Err(_) => self.try_lock(&probe).await,
            }
        } else {
            self.try_lock(&probe).await
        };
        let _ = self.delete(&probe).await;
        Ok(mode)
    }
    async fn try_lock(&self, name: &str) -> ConcurrencyMode {
        let method = Method::from_bytes(b"LOCK").unwrap();
        let body = r#"<?xml version="1.0"?><D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype><D:owner><D:href>Tracer</D:href></D:owner></D:lockinfo>"#;
        let response = self
            .auth(
                self.client
                    .request(method, self.url(name).unwrap())
                    .header("Timeout", "Second-120")
                    .header("Content-Type", "application/xml")
                    .body(body),
            )
            .send()
            .await;
        match response {
            Ok(r) if r.status().is_success() => {
                if let Some(token) = r.headers().get("Lock-Token").and_then(|v| v.to_str().ok()) {
                    let _ = self.unlock(name, token).await;
                    ConcurrencyMode::Lock
                } else {
                    ConcurrencyMode::None
                }
            }
            _ => ConcurrencyMode::None,
        }
    }
    pub async fn lock(&self, name: &str) -> Result<Option<String>> {
        let method = Method::from_bytes(b"LOCK").unwrap();
        let body = r#"<?xml version="1.0"?><D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype><D:owner><D:href>Tracer</D:href></D:owner></D:lockinfo>"#;
        let response = self
            .auth(
                self.client
                    .request(method, self.url(name)?)
                    .header("Timeout", "Second-120")
                    .header("Content-Type", "application/xml")
                    .body(body),
            )
            .send()
            .await
            .map_err(net)?;
        if !response.status().is_success() {
            return Err(status_error("加锁", response.status()));
        }
        Ok(response
            .headers()
            .get("Lock-Token")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned))
    }
    pub async fn unlock(&self, name: &str, token: &str) -> Result<()> {
        let method = Method::from_bytes(b"UNLOCK").unwrap();
        let response = self
            .auth(
                self.client
                    .request(method, self.url(name)?)
                    .header("Lock-Token", token),
            )
            .send()
            .await
            .map_err(net)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(status_error("解锁", response.status()))
        }
    }
}

fn net(e: reqwest::Error) -> SyncError {
    SyncError::Network(format!("WebDAV 请求失败: {}", e.without_url()))
}
fn status_error(action: &str, status: StatusCode) -> SyncError {
    SyncError::Network(format!("WebDAV {action}失败: HTTP {}", status.as_u16()))
}
