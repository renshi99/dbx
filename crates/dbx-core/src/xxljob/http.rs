use super::{XxlJobConfig, XxlJobRequest};
use crate::models::connection::ConnectionConfig;
use reqwest::{Client, Response, Url};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const RESPONSE_LIMIT: usize = 16 * 1024 * 1024;
type Form = Vec<(String, String)>;

struct Inner {
    http: Client,
    base: Url,
    cfg: XxlJobConfig,
    username: String,
    password: String,
    authenticated: Mutex<bool>,
    closed: CancellationToken,
    log_scope: Mutex<HashMap<i64, i32>>,
    log_requests: Mutex<HashMap<String, CancellationToken>>,
}

#[derive(Clone)]
pub struct XxlJobClient(Arc<Inner>);

impl XxlJobClient {
    pub fn new(config: &ConnectionConfig, transport: Option<(&str, u16)>) -> Result<Self, String> {
        let cfg = XxlJobConfig::from_connection(config)?;
        if config.username.trim().is_empty() || config.password.is_empty() {
            return Err("XXLJOB_AUTH: Username and password are required".into());
        }
        let mut base = cfg.base_url()?;
        let original = base.clone();
        let mut builder = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(cfg.tls_skip_verify);
        if let Some((host, port)) = transport {
            let address = std::net::SocketAddr::new(host.parse().map_err(|_| "Invalid tunnel address")?, port);
            let remote_host = base.host_str().unwrap().to_owned();
            if remote_host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok() {
                if base.scheme() == "https" && !cfg.tls_skip_verify {
                    return Err("XXLJOB_TLS: Use the certificate hostname for HTTPS tunnels".into());
                }
                base.set_host(Some(host)).map_err(|_| "Invalid tunnel host")?;
            } else {
                builder = builder.resolve(&remote_host, address);
            }
            base.set_port(Some(port)).map_err(|_| "Invalid tunnel port")?;
            let authority = original.as_str().split("://").nth(1).unwrap().split('/').next().unwrap();
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(reqwest::header::HOST, authority.parse().map_err(|_| "Invalid Host header")?);
            builder = builder.default_headers(headers).no_proxy();
        }
        Ok(Self(Arc::new(Inner {
            http: builder.build().map_err(|_| "Cannot create XXL-JOB client")?,
            base,
            cfg,
            username: config.username.trim().into(),
            password: config.password.clone(),
            authenticated: Mutex::new(false),
            closed: CancellationToken::new(),
            log_scope: Mutex::new(HashMap::new()),
            log_requests: Mutex::new(HashMap::new()),
        })))
    }

    async fn send(&self, path: &str, form: &[(String, String)], write: bool) -> Result<Response, String> {
        self.0
            .http
            .post(self.0.base.join(path).map_err(|_| "Invalid XXL-JOB endpoint")?)
            .form(form)
            .send()
            .await
            .map_err(|_| {
                if write {
                    "XXLJOB_UNCONFIRMED: Request outcome is unknown. Refresh tasks/logs before submitting again.".into()
                } else {
                    "XXLJOB_NETWORK: Request failed or timed out".into()
                }
            })
    }

    async fn login(&self) -> Result<(), String> {
        if self.0.closed.is_cancelled() {
            return Err("XXLJOB_CLOSED: Connection is closed".into());
        }
        let mut authenticated = self.0.authenticated.lock().await;
        if *authenticated {
            return Ok(());
        }
        let response = self
            .send(
                "login",
                &vec![("userName".into(), self.0.username.clone()), ("password".into(), self.0.password.clone())],
                false,
            )
            .await?;
        let has_cookie = response.cookies().any(|c| c.name() == "XXL_JOB_LOGIN_IDENTITY" && !c.value().is_empty());
        let value = decode(response, false)
            .await
            .map_err(|_| "XXLJOB_AUTH: Login failed; check native username/password and URL")?;
        if value["code"].as_i64() != Some(200) || !has_cookie {
            return Err("XXLJOB_AUTH: Invalid login or unsupported authentication response".into());
        }
        *authenticated = true;
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.0.closed.cancel();
        for token in self.0.log_requests.lock().await.values() {
            token.cancel();
        }
        self.0.log_scope.lock().await.clear();
    }

    async fn request(&self, path: &str, form: &[(String, String)], write: bool) -> Result<Value, String> {
        tokio::select! {
            biased;
            _ = self.0.closed.cancelled() => Err(if write {
                "XXLJOB_UNCONFIRMED: Connection closed; verify the result before submitting again".into()
            } else { "XXLJOB_CLOSED: Connection is closed".into() }),
            result = self.request_inner(path, form, write) => result,
        }
    }

    async fn request_inner(&self, path: &str, form: &[(String, String)], write: bool) -> Result<Value, String> {
        self.login().await?;
        let mut response = self.send(path, form, write).await?;
        if self.is_login_response(&response) {
            *self.0.authenticated.lock().await = false;
            if write {
                return Err("XXLJOB_AUTH: Session expired. Refresh before submitting again.".into());
            }
            self.login().await?;
            response = self.send(path, form, false).await?;
            if self.is_login_response(&response) {
                return Err("XXLJOB_AUTH: Session is not accepted".into());
            }
        }
        let value = decode(response, write).await?;
        if let Some(code) = value.get("code") {
            if code.as_i64() != Some(200) {
                return Err(format!("XXLJOB_BUSINESS: {}", value["msg"].as_str().unwrap_or("Operation failed")));
            }
        }
        Ok(value)
    }

    fn is_login_response(&self, response: &Response) -> bool {
        response.status().as_u16() == 401
            || (response.status().is_redirection()
                && response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|p| self.0.base.join(p).ok())
                    .is_some_and(|u| self.0.base.join("toLogin").is_ok_and(|expected| u == expected)))
    }

    pub async fn mutate(&self, path: &str, form: &[(String, String)]) -> Result<Value, String> {
        content(self.request(path, form, true).await?)
    }

    pub async fn probe(&self) -> Result<Value, String> {
        self.login().await?;
        if self.0.cfg.executor_mode == "manual" {
            for e in &self.0.cfg.executors {
                self.check_group(e.id).await?;
            }
        } else {
            self.executors(0).await?;
        }
        Ok(json!({"version": self.0.cfg.version, "executorMode": self.0.cfg.executor_mode,
            "url": self.0.cfg.server_addr}))
    }

    pub async fn executors(&self, page: u32) -> Result<Value, String> {
        if self.0.cfg.executor_mode == "manual" {
            let rows: Vec<_> = self.0.cfg.executors.iter().skip((page as usize).saturating_mul(20)).take(20)
                .map(|e| json!({"id": e.id, "title": if e.title.is_empty() { e.id.to_string() } else { e.title.clone() }, "manual": true})).collect();
            return Ok(json!({"items": rows, "total": self.0.cfg.executors.len()}));
        }
        let form = page_form(page)?;
        page_result(self.request("jobgroup/pageList", &form, false).await?)
    }

    pub async fn check_group(&self, group: i32) -> Result<(), String> {
        if group <= 0 || (self.0.cfg.executor_mode == "manual" && !self.0.cfg.executors.iter().any(|e| e.id == group)) {
            return Err("XXLJOB_PERMISSION: Executor is outside the configured scope".into());
        }
        // Unlike 2.4 jobinfo/pageList, joblog/pageList checks the live user's group permission.
        let form = vec![
            ("start".into(), "0".into()),
            ("length".into(), "1".into()),
            ("jobGroup".into(), group.to_string()),
            ("jobId".into(), "0".into()),
            ("logStatus".into(), "0".into()),
        ];
        page_result(self.request("joblog/pageList", &form, false).await?).map(|_| ())
    }

    pub async fn jobs(&self, req: &XxlJobRequest) -> Result<Value, String> {
        self.check_group(req.job_group).await?;
        let mut form = page_form(req.page)?;
        form.extend([
            ("jobGroup".into(), req.job_group.to_string()),
            ("triggerStatus".into(), req.trigger_status.unwrap_or(-1).to_string()),
            ("jobDesc".into(), req.job_desc.clone()),
            ("executorHandler".into(), req.executor_handler.clone()),
            ("author".into(), req.author.clone()),
        ]);
        page_result(self.request("jobinfo/pageList", &form, false).await?)
    }

    pub async fn job(&self, group: i32, id: i64) -> Result<Value, String> {
        if id <= 0 {
            return Err("XXL-JOB task ID is required".into());
        }
        self.check_group(group).await?;
        // The supported versions have no get-by-ID endpoint; use the scoped group endpoint.
        let rows = content(
            self.request("joblog/getJobsByGroup", &vec![("jobGroup".into(), group.to_string())], false).await?,
        )?;
        rows.as_array()
            .ok_or("XXLJOB_PROTOCOL: Invalid task list")?
            .iter()
            .find(|j| j["id"].as_i64() == Some(id) && j["jobGroup"].as_i64() == Some(i64::from(group)))
            .cloned()
            .ok_or("XXLJOB_PERMISSION: Task was removed or is outside this executor".into())
    }

    pub async fn next_times(&self, req: &XxlJobRequest) -> Result<Value, String> {
        content(
            self.request(
                "jobinfo/nextTriggerTime",
                &vec![
                    ("scheduleType".into(), req.schedule_type.clone()),
                    ("scheduleConf".into(), req.schedule_conf.clone()),
                ],
                false,
            )
            .await?,
        )
    }

    pub async fn logs(&self, req: &XxlJobRequest) -> Result<Value, String> {
        self.check_group(req.job_group).await?;
        let mut form = page_form(req.page)?;
        form.extend([
            ("jobGroup".into(), req.job_group.to_string()),
            ("jobId".into(), req.id.to_string()),
            ("logStatus".into(), req.log_status.to_string()),
            ("filterTime".into(), req.filter_time.clone()),
        ]);
        let result = page_result(self.request("joblog/pageList", &form, false).await?)?;
        let mut scope = self.0.log_scope.lock().await;
        if scope.len() > 10000 {
            scope.clear();
        }
        for row in result["items"].as_array().unwrap() {
            if row["jobGroup"].as_i64() == Some(i64::from(req.job_group)) {
                if let Some(id) = row["id"].as_i64() {
                    scope.insert(id, req.job_group);
                }
            }
        }
        Ok(result)
    }

    pub async fn cancel_log(&self, id: &str) {
        if let Some(token) = self.0.log_requests.lock().await.get(id) {
            token.cancel();
        }
    }

    pub async fn read_log(&self, req: &XxlJobRequest) -> Result<Value, String> {
        let operation = req
            .operation_id
            .as_deref()
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or("Log operation ID required")?;
        let token = CancellationToken::new();
        {
            let mut requests = self.0.log_requests.lock().await;
            if requests.contains_key(operation) {
                return Err("Duplicate log operation ID".into());
            }
            requests.insert(operation.into(), token.clone());
        }
        let result = tokio::select! {
            _ = token.cancelled() => Err("XXLJOB_CANCELLED: Log request cancelled".into()),
            result = self.read_log_inner(req) => result,
        };
        self.0.log_requests.lock().await.remove(operation);
        result
    }

    async fn read_log_inner(&self, req: &XxlJobRequest) -> Result<Value, String> {
        if self.0.log_scope.lock().await.get(&req.id).copied() != Some(req.job_group) {
            return Err("XXLJOB_PERMISSION: Refresh this executor's log list before opening a log".into());
        }
        self.check_group(req.job_group).await?;
        let from = req.from_line_num.unwrap_or(1);
        if from < 1 {
            return Err("Log line must be positive".into());
        }
        let value = content(
            self.request(
                "joblog/logDetailCat",
                &vec![("logId".into(), req.id.to_string()), ("fromLineNum".into(), from.to_string())],
                false,
            )
            .await?,
        )?;
        let text = value["logContent"].as_str().ok_or("XXLJOB_PROTOCOL: Missing log content")?;
        let to = value["toLineNum"]
            .as_i64()
            .filter(|v| *v >= 0 && *v < i64::from(i32::MAX))
            .ok_or("XXLJOB_PROTOCOL: Invalid log cursor")?;
        let end = value["isEnd"]
            .as_bool()
            .or_else(|| value["end"].as_bool())
            .ok_or("XXLJOB_PROTOCOL: Missing log end flag")?;
        Ok(json!({"text": text, "htmlEscaped": self.0.cfg.version == "2.5",
            "nextLine": (to + 1).max(i64::from(from)), "end": end}))
    }
}

fn page_form(page: u32) -> Result<Form, String> {
    let start = page.checked_mul(20).filter(|v| *v <= i32::MAX as u32).ok_or("Invalid page number")?;
    Ok(vec![("start".into(), start.to_string()), ("length".into(), "20".into())])
}

pub(super) fn page_result(value: Value) -> Result<Value, String> {
    let items = value["data"].as_array().ok_or("XXLJOB_PROTOCOL: Invalid paginated response")?;
    let total = value["recordsFiltered"].as_u64().ok_or("XXLJOB_PROTOCOL: Missing record count")?;
    Ok(json!({"items": items, "total": total}))
}

fn content(value: Value) -> Result<Value, String> {
    if value["code"].as_i64() != Some(200) {
        return Err("XXLJOB_PROTOCOL: Invalid operation response".into());
    }
    Ok(value.get("content").cloned().unwrap_or(Value::Null))
}

async fn decode(mut response: Response, write: bool) -> Result<Value, String> {
    let status = response.status();
    if status.as_u16() == 403 {
        return Err("XXLJOB_PERMISSION: Access denied".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        if write {
            "XXLJOB_UNCONFIRMED: Response interrupted; refresh before submitting again"
        } else {
            "XXLJOB_NETWORK: Response interrupted"
        }
    })? {
        if bytes.len() + chunk.len() > RESPONSE_LIMIT {
            return Err("XXLJOB_LIMIT: Response exceeds 16 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        return Err(if write {
            format!("XXLJOB_UNCONFIRMED: HTTP {}; refresh before submitting again", status.as_u16())
        } else {
            format!("XXLJOB_HTTP: HTTP {}; check permissions, native API URL and version", status.as_u16())
        });
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        if write {
            "XXLJOB_UNCONFIRMED: Invalid response; refresh before submitting again".into()
        } else {
            "XXLJOB_PROTOCOL: Expected JSON; check login, version and proxy URL".into()
        }
    })
}
