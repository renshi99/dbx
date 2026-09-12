use crate::models::connection::ConnectionConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutorScope {
    pub id: i32,
    #[serde(default)]
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XxlJobConfig {
    pub server_addr: String,
    pub version: String,
    #[serde(default)]
    pub tls_skip_verify: bool,
    pub executor_mode: String,
    #[serde(default)]
    pub executors: Vec<ExecutorScope>,
}

impl XxlJobConfig {
    pub fn from_connection(config: &ConnectionConfig) -> Result<Self, String> {
        let cfg: Self =
            serde_json::from_value(config.external_config.clone().ok_or("XXL-JOB configuration is required")?)
                .map_err(|_| "Invalid XXL-JOB configuration")?;
        cfg.base_url()?;
        if !matches!(cfg.version.as_str(), "2.4" | "2.5") {
            return Err("Choose XXL-JOB 2.4.x or 2.5.x".into());
        }
        if !matches!(cfg.executor_mode.as_str(), "automatic" | "manual") {
            return Err("Invalid XXL-JOB executor discovery mode".into());
        }
        if cfg.executor_mode == "manual"
            && (cfg.executors.is_empty()
                || cfg.executors.len() > 100
                || cfg.executors.iter().any(|e| e.id <= 0)
                || cfg.executors.iter().map(|e| e.id).collect::<std::collections::HashSet<_>>().len()
                    != cfg.executors.len())
        {
            return Err("Configure 1–100 unique, positive executor IDs".into());
        }
        Ok(cfg)
    }

    pub fn base_url(&self) -> Result<reqwest::Url, String> {
        let mut url = reqwest::Url::parse(self.server_addr.trim()).map_err(|_| "Invalid XXL-JOB URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("XXL-JOB URL must be HTTP(S), without credentials, query or fragment".into());
        }
        url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
        Ok(url)
    }
}
