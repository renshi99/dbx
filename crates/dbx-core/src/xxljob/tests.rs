use super::*;
use crate::models::connection::ConnectionConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn config(base: &str, version: &str, manual: bool) -> ConnectionConfig {
    serde_json::from_value(json!({
        "id":"xxl-test", "name":"XXL test", "db_type":"xxljob", "host":"127.0.0.1", "port":8080,
        "username":"admin", "password":"secret", "external_config":{
            "serverAddr":base, "version":version, "executorMode":if manual {"manual"} else {"automatic"},
            "executors":[{"id":1,"title":"Example"}]
        }
    }))
    .unwrap()
}
fn response(status: &str, headers: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len())
}
fn ok(body: &str) -> String {
    response("200 OK", "Content-Type: application/json\r\n", body)
}
fn login() -> String {
    response(
        "200 OK",
        "Set-Cookie: XXL_JOB_LOGIN_IDENTITY=test-session; Path=/scheduler/\r\n",
        r#"{"code":200,"content":null}"#,
    )
}
fn empty() -> String {
    ok(r#"{"data":[],"recordsFiltered":0,"recordsTotal":0}"#)
}
async fn server(responses: Vec<String>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/scheduler/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        for response in responses {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept()).await.unwrap().unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let size = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + size {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(request).unwrap());
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        requests
    });
    (base, task)
}

#[tokio::test]
async fn shutdown_prevents_reuse_of_cached_sessions() {
    let (base, task) = server(vec![login(), empty()]).await;
    let client = XxlJobClient::new(&config(&base, "2.5", false), None).unwrap();
    client.probe().await.unwrap();
    task.await.unwrap();
    client.shutdown().await;
    assert!(client.executors(0).await.unwrap_err().contains("CLOSED"));
    assert!(client.probe().await.unwrap_err().contains("CLOSED"));
}

#[test]
fn configuration_rejects_invalid_urls_and_scopes() {
    for base in ["file:///tmp/admin", "https://user:secret@admin/", "https://admin/?token=secret", "https://admin/#x"] {
        assert!(XxlJobConfig::from_connection(&config(base, "2.5", false)).is_err());
    }
    assert!(XxlJobConfig::from_connection(&config("https://admin/", "3.0", false)).is_err());
    let mut cfg = config("https://admin/xxl-job-admin", "2.4", true);
    cfg.external_config.as_mut().unwrap()["executors"] = json!([{"id":1},{"id":1}]);
    assert!(XxlJobConfig::from_connection(&cfg).is_err());
}

#[tokio::test]
async fn both_versions_login_keep_cookie_and_honor_pagination_and_prefix() {
    for version in ["2.4", "2.5"] {
        let (base, task) = server(vec![login(), empty(), empty()]).await;
        let client = XxlJobClient::new(&config(&base, version, false), None).unwrap();
        assert_eq!(client.probe().await.unwrap()["version"], version);
        client.executors(2).await.unwrap();
        let requests = task.await.unwrap();
        assert!(requests[0].starts_with("POST /scheduler/login "));
        assert!(requests[0].contains("userName=admin&password=secret"));
        assert!(requests[1].to_lowercase().contains("cookie: xxl_job_login_identity=test-session"));
        assert!(requests[2].contains("start=40&length=20"));
        assert_eq!(requests.iter().filter(|r| r.starts_with("POST /scheduler/login ")).count(), 1);
    }
}

#[tokio::test]
async fn query_reauthenticates_once_but_mutations_never_replay() {
    let expired = response("302 Found", "Location: /scheduler/toLogin\r\n", "");
    let (base, task) = server(vec![login(), expired.clone(), login(), empty(), expired]).await;
    let client = XxlJobClient::new(&config(&base, "2.5", false), None).unwrap();
    client.executors(0).await.unwrap();
    let error = client.mutate("jobinfo/trigger", &vec![("id".into(), "1".into())]).await.unwrap_err();
    assert!(error.contains("Session expired"));
    let requests = task.await.unwrap();
    assert_eq!(requests.iter().filter(|r| r.starts_with("POST /scheduler/jobinfo/trigger ")).count(), 1);
}

#[tokio::test]
async fn permission_failure_is_not_an_empty_page_or_auth_retry() {
    let (base, task) = server(vec![login(), response("403 Forbidden", "", "")]).await;
    let client = XxlJobClient::new(&config(&base, "2.4", false), None).unwrap();
    assert!(client.executors(0).await.unwrap_err().contains("PERMISSION"));
    assert_eq!(task.await.unwrap().len(), 2);
}

#[tokio::test]
async fn manual_scope_checks_live_permission_and_blocks_other_groups_without_request() {
    let (base, task) = server(vec![login(), empty()]).await;
    let client = XxlJobClient::new(&config(&base, "2.5", true), None).unwrap();
    client.probe().await.unwrap();
    assert!(client.check_group(2).await.unwrap_err().contains("scope"));
    assert_eq!(client.executors(0).await.unwrap()["items"][0]["id"], 1);
    let requests = task.await.unwrap();
    assert!(requests[1].starts_with("POST /scheduler/joblog/pageList "));
    assert!(requests[1].contains("jobGroup=1"));
}

#[tokio::test]
async fn failed_or_uncertain_writes_are_not_retried() {
    let (base, task) =
        server(vec![login(), response("503 Service Unavailable", "", ""), ok(r#"{"code":500,"msg":"bad cron"}"#)])
            .await;
    let client = XxlJobClient::new(&config(&base, "2.5", false), None).unwrap();
    assert!(client.mutate("jobinfo/start", &[]).await.unwrap_err().contains("UNCONFIRMED"));
    assert!(client.mutate("jobinfo/update", &[]).await.unwrap_err().contains("bad cron"));
    assert_eq!(task.await.unwrap().len(), 3);
}

#[tokio::test]
async fn task_membership_is_verified() {
    let (base, task) = server(vec![login(), empty(), ok(r#"{"code":200,"content":[{"id":4,"jobGroup":2}]}"#)]).await;
    let client = XxlJobClient::new(&config(&base, "2.4", true), None).unwrap();
    assert!(client.job(1, 4).await.unwrap_err().contains("outside"));
    assert_eq!(task.await.unwrap().len(), 3);
}

#[tokio::test]
async fn log_reads_require_scoped_list_and_preserve_version_metadata() {
    for version in ["2.4", "2.5"] {
        let (base, task) = server(vec![
            login(),
            empty(),
            ok(r#"{"data":[{"id":10,"jobGroup":1}],"recordsFiltered":1}"#),
            empty(),
            ok(r#"{"code":200,"content":{"fromLineNum":1,"toLineNum":2,"logContent":"&lt;script&gt;","end":true}}"#),
        ])
        .await;
        let client = XxlJobClient::new(&config(&base, version, true), None).unwrap();
        let req = XxlJobRequest { id: 10, job_group: 1, operation_id: Some("log-1".into()), ..Default::default() };
        assert!(client.read_log(&req).await.unwrap_err().contains("Refresh"));
        client.logs(&XxlJobRequest { job_group: 1, ..Default::default() }).await.unwrap();
        let chunk = client.read_log(&req).await.unwrap();
        assert_eq!(chunk["nextLine"], 3);
        assert_eq!(chunk["end"], true);
        assert_eq!(chunk["htmlEscaped"], version == "2.5");
        assert_eq!(chunk["text"], "&lt;script&gt;");
        assert!(task.await.unwrap()[4].contains("logId=10&fromLineNum=1"));
    }
}

#[test]
fn update_dto_excludes_glue_source_and_runtime_state() {
    let draft: JobDraft = serde_json::from_value(json!({
        "id":7,"jobGroup":1,"jobDesc":"job","author":"owner","alarmEmail":"",
        "scheduleType":"CRON","scheduleConf":"0 * * * * ?","misfireStrategy":"DO_NOTHING",
        "executorRouteStrategy":"FIRST","executorHandler":"handler","executorParam":"x&y",
        "executorBlockStrategy":"SERIAL_EXECUTION","executorTimeout":0,"executorFailRetryCount":0,
        "glueType":"BEAN","childJobId":"","glueSource":"do not send", "triggerStatus":1
    }))
    .unwrap();
    draft.validate().unwrap();
    let form = draft.form();
    assert!(form.contains(&("id".into(), "7".into())));
    assert!(form.contains(&("scheduleConf".into(), "0 * * * * ?".into())));
    assert!(!form.iter().any(|(k, _)| k == "glueSource" || k == "triggerStatus"));
    assert!(form.contains(&("executorParam".into(), "x&y".into())));
}

#[tokio::test]
async fn readonly_blocks_every_mutation_before_connection() {
    let dir = tempfile::tempdir().unwrap();
    let storage = crate::storage::Storage::open(&dir.path().join("storage.db")).await.unwrap();
    let state = AppState::new_with_plugin_dir(storage, dir.path().join("plugins"));
    let mut cfg = config("http://127.0.0.1:1/admin", "2.5", true);
    cfg.read_only = true;
    state.configs.write().await.insert(cfg.id.clone(), cfg.clone());
    for operation in ["addJob", "updateJob", "removeJob", "startJob", "stopJob", "triggerJob"] {
        let error = execute(&state, operation, XxlJobRequest { connection_id: cfg.id.clone(), ..Default::default() })
            .await
            .unwrap_err();
        assert!(error.contains("READ_ONLY"), "{operation}: {error}");
    }
}
