//! XXL-JOB 2.4/2.5 management API, shared by desktop and Web.
pub mod config;
mod http;
pub mod types;
use crate::connection::{AppState, PoolKind};
pub use config::XxlJobConfig;
pub use http::XxlJobClient;
use serde_json::{json, Value};
pub use types::*;

pub fn database_info(info: &Value) -> crate::models::connection::DatabaseConnectionInfo {
    crate::models::connection::DatabaseConnectionInfo {
        product_name: Some("XXL-JOB".into()),
        // Configured compatibility version is not a detected server version.
        server_comment: info["version"].as_str().map(|v| format!("XXL-JOB {v}.x compatibility")),
        driver_name: Some("XXL-JOB HTTP API".into()),
        ..Default::default()
    }
}

pub async fn execute(state: &AppState, operation: &str, req: XxlJobRequest) -> Result<Value, String> {
    if !matches!(
        operation,
        "testConnection"
            | "listExecutors"
            | "listJobs"
            | "addJob"
            | "updateJob"
            | "removeJob"
            | "startJob"
            | "stopJob"
            | "triggerJob"
            | "nextTriggerTime"
            | "listLogs"
            | "readLog"
            | "cancelLog"
    ) {
        return Err("Unknown XXL-JOB operation".into());
    }
    let cfg = state.configs.read().await.get(&req.connection_id).cloned().ok_or("Connection not found")?;
    if cfg.db_type != crate::models::connection::DatabaseType::XxlJob {
        return Err("Connection is not an XXL-JOB connection".into());
    }
    if matches!(operation, "addJob" | "updateJob" | "removeJob" | "startJob" | "stopJob" | "triggerJob")
        && crate::query::connection_readonly_name(state, &req.connection_id).await.is_some()
    {
        return Err("XXLJOB_READ_ONLY: Connection is read-only".into());
    }
    // Cancellation must never reconnect a closed connection.
    if operation != "cancelLog" {
        state.get_or_create_pool(&req.connection_id, None).await?;
    }
    let client = match state.pool_handle(&req.connection_id).await.as_ref() {
        Some(PoolKind::XxlJob(client)) => client.clone(),
        _ if operation == "cancelLog" => return Ok(Value::Null),
        _ => return Err("XXL-JOB client unavailable".into()),
    };
    match operation {
        "testConnection" => client.probe().await,
        "listExecutors" => client.executors(req.page).await,
        "listJobs" => client.jobs(&req).await,
        "nextTriggerTime" => client.next_times(&req).await,
        "listLogs" => client.logs(&req).await,
        "readLog" => client.read_log(&req).await,
        "cancelLog" => {
            client.cancel_log(req.operation_id.as_deref().ok_or("Missing operation ID")?).await;
            Ok(Value::Null)
        }
        "addJob" | "updateJob" => {
            let draft = req.draft.as_ref().ok_or("Missing task form")?;
            draft.validate()?;
            client.check_group(draft.job_group).await?;
            if operation == "addJob" {
                if draft.glue_type != "BEAN" || draft.id != 0 {
                    return Err("New tasks must use BEAN mode".into());
                }
            } else {
                let old = client.job(draft.job_group, i64::from(draft.id)).await?;
                if old["glueType"].as_str() != Some(&draft.glue_type) {
                    return Err("Cannot change GLUE type".into());
                }
            }
            client.mutate(if operation == "addJob" { "jobinfo/add" } else { "jobinfo/update" }, &draft.form()).await
        }
        "removeJob" | "startJob" | "stopJob" | "triggerJob" => {
            let job = client.job(req.job_group, req.id).await?;
            let mut form = vec![("id".into(), req.id.to_string())];
            let path = match operation {
                "removeJob" => "jobinfo/remove",
                "startJob" => "jobinfo/start",
                "stopJob" => "jobinfo/stop",
                _ => {
                    form.push((
                        "executorParam".into(),
                        req.executor_param
                            .clone()
                            .unwrap_or_else(|| job["executorParam"].as_str().unwrap_or_default().to_string()),
                    ));
                    form.push(("addressList".into(), req.address_list.clone()));
                    "jobinfo/trigger"
                }
            };
            let result = client.mutate(path, &form).await?;
            Ok(if operation == "triggerJob" { json!({"accepted": true}) } else { result })
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests;
