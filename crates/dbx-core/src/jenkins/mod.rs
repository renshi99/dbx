//! Jenkins Remote Access API. All URLs are constructed from the saved connection.
use std::collections::HashMap;
use std::time::Duration;

use reqwest::{Client, Method, Response, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::connection::{AppState, PoolKind};
use crate::models::connection::ConnectionConfig;

#[cfg(test)]
mod tests;

const LOG_LIMIT: usize = 1024 * 1024;
const JSON_LIMIT: usize = 16 * 1024 * 1024;

pub fn database_info(info: &Value) -> crate::models::connection::DatabaseConnectionInfo {
    crate::models::connection::DatabaseConnectionInfo {
        product_name: Some("Jenkins".into()),
        product_version: info["version"].as_str().map(str::to_owned),
        driver_name: Some("Jenkins HTTP API".into()),
        ..Default::default()
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsConfig {
    pub server_addr: String,
    #[serde(default)]
    pub tls_skip_verify: bool,
}

impl JenkinsConfig {
    pub fn from_connection(config: &ConnectionConfig) -> Result<Self, String> {
        let result: Self = serde_json::from_value(config.external_config.clone().ok_or("Jenkins URL is required")?)
            .map_err(|_| "Invalid Jenkins connection configuration")?;
        result.base_url()?;
        Ok(result)
    }

    fn base_url(&self) -> Result<Url, String> {
        let mut url = Url::parse(self.server_addr.trim()).map_err(|_| "Invalid Jenkins URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Jenkins URL must be HTTP(S), without credentials, query or fragment".into());
        }
        url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
        Ok(url)
    }
}

#[derive(Clone)]
pub struct JenkinsClient {
    http: Client,
    base: Url,
    display_base: Url,
    username: String,
    token: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsRequest {
    pub connection_id: String,
    #[serde(default)]
    pub path: Vec<String>,
    pub number: Option<u64>,
    pub queue_id: Option<u64>,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub start: u64,
    #[serde(default)]
    pub parameters: HashMap<String, Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsLog {
    pub text: String,
    pub next_start: u64,
    pub more: bool,
}

impl JenkinsClient {
    pub fn new(config: &ConnectionConfig, connect_override: Option<(&str, u16)>) -> Result<Self, String> {
        let cfg = JenkinsConfig::from_connection(config)?;
        let mut base = cfg.base_url()?;
        let display_base = base.clone();
        if !config.username.trim().is_empty() && config.password.is_empty() {
            return Err("Jenkins API Token is required".into());
        }
        let mut builder = Client::builder()
            .use_rustls_tls()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(cfg.tls_skip_verify);
        // Retain the remote virtual host and, for DNS names, TLS identity.
        if let Some((host, port)) = connect_override {
            let address = std::net::SocketAddr::new(host.parse().map_err(|_| "Invalid Jenkins tunnel address")?, port);
            let original_host = base.host_str().unwrap().to_owned();
            if original_host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok() {
                if base.scheme() == "https" && !cfg.tls_skip_verify {
                    return Err("JENKINS_TLS: Use the server's certificate hostname for an HTTPS tunnel".into());
                }
                base.set_host(Some(host)).map_err(|_| "Invalid Jenkins tunnel host")?;
            } else {
                builder = builder.resolve(&original_host, address);
            }
            base.set_port(Some(port)).map_err(|_| "Invalid Jenkins tunnel port")?;
            let mut headers = reqwest::header::HeaderMap::new();
            let authority = display_base.as_str().split("://").nth(1).unwrap().split('/').next().unwrap();
            headers.insert(reqwest::header::HOST, authority.parse().map_err(|_| "Invalid Jenkins Host header")?);
            builder = builder.default_headers(headers).no_proxy();
        }
        Ok(Self {
            http: builder.build().map_err(|_| "Cannot create Jenkins HTTP client")?,
            base,
            display_base,
            username: config.username.trim().into(),
            token: config.password.clone(),
        })
    }

    fn endpoint(&self, path: &[String], suffix: &[&str]) -> Result<Url, String> {
        let mut url = self.base.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|_| "Invalid Jenkins base URL")?;
            segments.pop_if_empty();
            for part in path {
                if part.is_empty() || matches!(part.as_str(), "." | "..") {
                    return Err("Invalid Jenkins job path".into());
                }
                segments.push("job").push(part);
            }
            for part in suffix {
                segments.push(part);
            }
        }
        Ok(url)
    }

    async fn send(&self, method: Method, url: Url, form: Option<&[(String, String)]>) -> Result<Response, String> {
        let write = method == Method::POST;
        let mut request = self.http.request(method, url);
        if !self.username.is_empty() {
            request = request.basic_auth(&self.username, Some(&self.token));
        }
        if let Some(form) = form {
            request = request.form(form);
        }
        let response = request.send().await.map_err(|_| {
            if write {
                "JENKINS_UNCONFIRMED: Request outcome is unknown. Refresh queue/history before submitting again."
            } else {
                "JENKINS_NETWORK: Request failed or timed out"
            }
        })?;
        match response.status().as_u16() {
            200..=299 => Ok(response),
            // Jenkins stop/cancel can redirect after accepting the POST. Do not follow it.
            302 | 303
                if write
                    && response
                        .headers()
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                        .is_some_and(|location| self.safe_redirect(location)) =>
            {
                Ok(response)
            }
            401 => Err("JENKINS_AUTH: Invalid username or API Token".into()),
            403 => Err("JENKINS_FORBIDDEN: Permission denied or CSRF rejected; use a valid API Token".into()),
            404 => Err("JENKINS_NOT_FOUND: Job, build, queue item or API path is unavailable".into()),
            300..=399 => Err("JENKINS_REDIRECT: Check the base URL and API authentication (SSO is unsupported)".into()),
            500..=599 if write => {
                Err("JENKINS_UNCONFIRMED: Server failed; refresh queue/history before submitting again".into())
            }
            code => Err(format!("JENKINS_HTTP: Server returned HTTP {code}")),
        }
    }

    fn safe_redirect(&self, location: &str) -> bool {
        self.display_base.join(location).is_ok_and(|url| {
            (url.origin() == self.base.origin() || url.origin() == self.display_base.origin())
                && url.path().starts_with(self.display_base.path())
                && !url.path().contains("/login")
        })
    }

    async fn json(&self, path: &[String], suffix: &[&str], tree: Option<&str>) -> Result<Value, String> {
        let mut url = self.endpoint(path, suffix)?;
        if let Some(tree) = tree {
            url.query_pairs_mut().append_pair("tree", tree);
        }
        let response = self.send(Method::GET, url, None).await?;
        let (bytes, truncated) = read_bytes(response, JSON_LIMIT).await?;
        if truncated {
            return Err("JENKINS_TOO_LARGE: API response exceeds 16 MiB; open a smaller folder".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            "JENKINS_RESPONSE: Expected JSON; check URL and authentication (HTML login pages are unsupported)".into()
        })
    }

    pub async fn probe(&self) -> Result<Value, String> {
        let mut url = self.endpoint(&[], &["api", "json"])?;
        url.query_pairs_mut().append_pair("tree", "mode,nodeDescription,jobs[name,_class,color]");
        let response = self.send(Method::GET, url, None).await?;
        let version = response.headers().get("x-jenkins").and_then(|v| v.to_str().ok()).map(str::to_owned);
        let (bytes, truncated) = read_bytes(response, JSON_LIMIT).await?;
        if truncated {
            return Err("Jenkins root response is too large".into());
        }
        let data: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Jenkins returned a non-JSON response; check URL and authentication")?;
        if data.get("jobs").is_none() && data.get("mode").is_none() {
            return Err("Response is not a Jenkins controller API".into());
        }
        Ok(json!({ "version": version, "anonymous": self.username.is_empty(), "url": self.display_base.as_str() }))
    }

    async fn job(&self, path: &[String]) -> Result<Value, String> {
        self.json(path, &["api", "json"], Some("name,displayName,_class,color,buildable,inQueue,queueItem[id,why,cancelled,executable[number]],nextBuildNumber,property[parameterDefinitions[name,type,_class,description,choices,defaultParameterValue[value]]],lastBuild[number,result,building]")).await
    }

    async fn log(&self, req: &JenkinsRequest) -> Result<JenkinsLog, String> {
        let number = required_id(req.number)?.to_string();
        let mut url = self.endpoint(&req.path, &[&number, "logText", "progressiveText"])?;
        url.query_pairs_mut().append_pair("start", &req.start.to_string());
        let response = self.send(Method::GET, url, None).await?;
        let server_more = response.headers().get("x-more-data").is_some_and(|v| v == "true");
        let server_end =
            response.headers().get("x-text-size").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
        if server_end.is_some_and(|end| end < req.start) {
            return Err("JENKINS_LOG_RESET: Log was reset; refresh to read from the beginning".into());
        }
        let (mut bytes, truncated) = read_bytes(response, LOG_LIMIT).await?;
        // Replay an incomplete trailing UTF-8 character on the next request.
        if truncated {
            if let Err(error) = std::str::from_utf8(&bytes) {
                if error.error_len().is_none() {
                    bytes.truncate(error.valid_up_to());
                }
            }
        }
        let consumed = bytes.len() as u64;
        let next_start = if truncated { req.start + consumed } else { server_end.unwrap_or(req.start + consumed) };
        Ok(JenkinsLog {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            next_start,
            more: server_more || truncated,
        })
    }

    async fn trigger(&self, req: &JenkinsRequest) -> Result<Value, String> {
        let job = self.job(&req.path).await?;
        if job["buildable"] != true {
            return Err("Jenkins job is not buildable".into());
        }
        let definitions = parameter_definitions(&job);
        let mut form = Vec::new();
        for definition in &definitions {
            let name = definition["name"].as_str().ok_or("Invalid Jenkins parameter name")?;
            let kind = parameter_kind(definition);
            let value = req.parameters.get(name).or_else(|| definition.pointer("/defaultParameterValue/value"));
            let encoded = match kind {
                "BooleanParameterDefinition" => {
                    value.and_then(Value::as_bool).ok_or("Boolean parameter required")?.to_string()
                }
                "StringParameterDefinition" | "TextParameterDefinition" => match value {
                    Some(value) => value.as_str().ok_or("String parameter required")?.to_owned(),
                    None => String::new(),
                },
                "ChoiceParameterDefinition" => {
                    let value = value.and_then(Value::as_str).ok_or("Choice parameter required")?;
                    if !definition["choices"].as_array().is_some_and(|choices| choices.iter().any(|item| item == value))
                    {
                        return Err("Invalid Jenkins choice parameter".into());
                    }
                    value.to_owned()
                }
                _ => return Err("JENKINS_PARAMETER: Unsupported parameter type; run this job in Jenkins".into()),
            };
            form.push((name.to_owned(), encoded));
        }
        if req.parameters.keys().any(|name| !definitions.iter().any(|d| d["name"].as_str() == Some(name))) {
            return Err("Unknown Jenkins parameter".into());
        }
        let action = if definitions.is_empty() { "build" } else { "buildWithParameters" };
        let response = self.send(Method::POST, self.endpoint(&req.path, &[action])?, Some(&form)).await?;
        if response.status().is_redirection() {
            return Err("JENKINS_UNCONFIRMED: Build request redirected; refresh history before retrying".into());
        }
        let queue_id =
            response.headers().get("location").and_then(|v| v.to_str().ok()).and_then(queue_id_from_location);
        Ok(json!({ "accepted": true, "queueId": queue_id }))
    }
}

fn required_id(value: Option<u64>) -> Result<u64, String> {
    value.filter(|v| *v > 0).ok_or("Jenkins ID is required".into())
}

pub fn parameter_definitions(job: &Value) -> Vec<Value> {
    job["property"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|p| p["parameterDefinitions"].as_array().into_iter().flatten().cloned())
        .collect()
}

fn parameter_kind(definition: &Value) -> &str {
    definition["_class"].as_str().or_else(|| definition["type"].as_str()).unwrap_or("").rsplit('.').next().unwrap_or("")
}

fn queue_id_from_location(location: &str) -> Option<u64> {
    let base = Url::parse("http://jenkins.invalid/").ok()?;
    let url = base.join(location).ok()?;
    let segments: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let tail = segments.get(segments.len().checked_sub(3)?..)?;
    if tail[0] != "queue" || tail[1] != "item" {
        return None;
    }
    tail[2].parse::<u64>().ok().filter(|id| *id > 0)
}

async fn read_bytes(mut response: Response, limit: usize) -> Result<(Vec<u8>, bool), String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "Jenkins response was interrupted")? {
        let remaining = limit - bytes.len();
        if chunk.len() > remaining {
            bytes.extend_from_slice(&chunk[..remaining]);
            return Ok((bytes, true));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((bytes, false))
}

/// Single dispatcher shared by desktop and Web; operation names are an explicit allowlist.
pub async fn execute(state: &AppState, operation: &str, req: JenkinsRequest) -> Result<Value, String> {
    if !matches!(
        operation,
        "testConnection"
            | "listJobs"
            | "getJob"
            | "listBuilds"
            | "getBuild"
            | "getBuildLog"
            | "triggerBuild"
            | "getQueueItem"
            | "cancelQueueItem"
            | "stopBuild"
    ) {
        return Err("Unknown Jenkins operation".into());
    }
    let config = state.configs.read().await.get(&req.connection_id).cloned().ok_or("Connection not found")?;
    if config.db_type != crate::models::connection::DatabaseType::Jenkins {
        return Err("Connection is not a Jenkins connection".into());
    }
    let write = matches!(operation, "triggerBuild" | "cancelQueueItem" | "stopBuild");
    if write {
        if crate::query::connection_readonly_name(state, &req.connection_id).await.is_some() {
            return Err("JENKINS_READ_ONLY: Connection is read-only".into());
        }
    }
    state.get_or_create_pool(&req.connection_id, None).await?;
    let client = match state.pool_handle(&req.connection_id).await.as_ref() {
        Some(PoolKind::Jenkins(client)) => client.clone(),
        _ => return Err("Connection is not a Jenkins connection".into()),
    };
    if write && client.username.is_empty() {
        return Err("JENKINS_READ_ONLY: Anonymous connections are read-only".into());
    }
    match operation {
        "testConnection" => client.probe().await,
        "listJobs" => {
            client
                .json(
                    &req.path,
                    &["api", "json"],
                    Some("jobs[name,displayName,_class,color,buildable,lastBuild[number,result,building]]"),
                )
                .await
        }
        "getJob" => client.job(&req.path).await,
        "listBuilds" => {
            let start = u64::from(req.page) * 50;
            client
                .json(
                    &req.path,
                    &["api", "json"],
                    Some(&format!(
                        "builds[number,result,building,timestamp,duration,displayName]{{{start},{}}}",
                        start + 50
                    )),
                )
                .await
        }
        "getBuild" => {
            client
                .json(
                    &req.path,
                    &[&required_id(req.number)?.to_string(), "api", "json"],
                    Some("number,result,building,timestamp,duration,displayName,queueId"),
                )
                .await
        }
        "getBuildLog" => serde_json::to_value(client.log(&req).await?).map_err(|_| "Cannot encode Jenkins log".into()),
        "triggerBuild" => client.trigger(&req).await,
        "getQueueItem" => {
            client
                .json(
                    &[],
                    &["queue", "item", &required_id(req.queue_id)?.to_string(), "api", "json"],
                    Some("id,cancelled,why,blocked,buildable,executable[number]"),
                )
                .await
        }
        "cancelQueueItem" => {
            let mut url = client.endpoint(&[], &["queue", "cancelItem"])?;
            url.query_pairs_mut().append_pair("id", &required_id(req.queue_id)?.to_string());
            client.send(Method::POST, url, Some(&[])).await?;
            Ok(Value::Null)
        }
        "stopBuild" => {
            client
                .send(
                    Method::POST,
                    client.endpoint(&req.path, &[&required_id(req.number)?.to_string(), "stop"])?,
                    Some(&[]),
                )
                .await?;
            Ok(Value::Null)
        }
        _ => Err("Unknown Jenkins operation".into()),
    }
}
