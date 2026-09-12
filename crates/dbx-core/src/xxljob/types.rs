use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDraft {
    #[serde(default)]
    pub id: i32,
    pub job_group: i32,
    pub job_desc: String,
    pub author: String,
    pub alarm_email: String,
    pub schedule_type: String,
    pub schedule_conf: String,
    pub misfire_strategy: String,
    pub executor_route_strategy: String,
    pub executor_handler: String,
    pub executor_param: String,
    pub executor_block_strategy: String,
    pub executor_timeout: i32,
    pub executor_fail_retry_count: i32,
    pub glue_type: String,
    pub child_job_id: String,
}

impl JobDraft {
    pub fn validate(&self) -> Result<(), String> {
        if self.job_group <= 0
            || self.job_desc.trim().is_empty()
            || self.author.trim().is_empty()
            || self.executor_timeout < 0
            || self.executor_fail_retry_count < 0
        {
            return Err(
                "XXL-JOB: executor, description and author are required; timeout/retries cannot be negative".into()
            );
        }
        if self.glue_type == "BEAN" && self.executor_handler.trim().is_empty() {
            return Err("XXL-JOB: BEAN Handler is required".into());
        }
        Ok(())
    }

    pub fn form(&self) -> Vec<(String, String)> {
        // The explicit DTO excludes GLUE source, timestamps and scheduler state.
        serde_json::to_value(self)
            .unwrap()
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())))
            .collect()
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XxlJobRequest {
    pub connection_id: String,
    #[serde(default)]
    pub job_group: i32,
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub job_desc: String,
    #[serde(default)]
    pub executor_handler: String,
    #[serde(default)]
    pub author: String,
    pub trigger_status: Option<i32>,
    #[serde(default)]
    pub log_status: i32,
    #[serde(default)]
    pub filter_time: String,
    pub executor_param: Option<String>,
    #[serde(default)]
    pub address_list: String,
    #[serde(default)]
    pub schedule_type: String,
    #[serde(default)]
    pub schedule_conf: String,
    pub from_line_num: Option<i32>,
    pub draft: Option<JobDraft>,
    pub operation_id: Option<String>,
}
