use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CHECKBOX_FORM: &str = include_str!("checkbox-form.html");

fn checkbox_job() -> Value {
    json!({"buildable": true, "property": [{"parameterDefinitions": [
        {"name": "TARGET", "_class": "hudson.model.ChoiceParameterDefinition", "choices": ["dev", "stage"]},
        {"name": "MODULES", "_class": "com.cwctravel.hudson.plugins.extended_choice_parameter.ExtendedChoiceParameterDefinition", "type": "PT_CHECKBOX"},
        {"name": "Branch", "_class": "hudson.model.StringParameterDefinition"},
        {"name": "FLAG", "_class": "hudson.model.BooleanParameterDefinition"},
        {"name": "NOTES", "_class": "hudson.model.TextParameterDefinition"}
    ]}]})
}

fn checkbox_revision() -> String {
    let mut job = checkbox_job();
    parameters::enrich(&mut job, CHECKBOX_FORM).unwrap();
    format!("{:x}", Sha256::digest(serde_json::to_vec(&parameter_definitions(&job)).unwrap()))
}

#[test]
fn parses_native_form_defaults_and_entities_without_running_scripts() {
    let mut job = checkbox_job();
    parameters::enrich(&mut job, CHECKBOX_FORM).unwrap();
    let definitions = parameter_definitions(&job);
    assert_eq!(definitions[0]["defaultParameterValue"]["value"], "stage");
    assert_eq!(definitions[1]["choices"], json!(["gateway", "支付&清算"]));
    assert_eq!(definitions[1]["defaultParameterValue"]["value"], json!(["支付&清算"]));
    assert_eq!(definitions[2]["defaultParameterValue"]["value"], "test");
    assert_eq!(definitions[3]["defaultParameterValue"]["value"], true);
    assert_eq!(definitions[4]["defaultParameterValue"]["value"], "构建 <说明>");
    parameters::enrich(&mut job, &CHECKBOX_FORM.replace(" checked", "")).unwrap();
    assert_eq!(parameter_definitions(&job)[1]["defaultParameterValue"]["value"], json!([]));
}

#[test]
fn refuses_missing_ambiguous_and_unsupported_form_fields() {
    for html in ["<form method='post'>Login</form>".to_owned(), CHECKBOX_FORM.replace("MODULES.value", "wrong"),
        CHECKBOX_FORM.replace("value=\"MODULES\"", "value=\"TARGET\""), CHECKBOX_FORM.replace("name=\"value\" value=\"test\"", "name=\"missing\""),
        format!("{CHECKBOX_FORM}{CHECKBOX_FORM}")] {
        assert!(parameters::enrich(&mut checkbox_job(), &html).is_err());
    }
    let mut job = checkbox_job();
    job["property"][0]["parameterDefinitions"][1]["type"] = json!("PT_MULTI_SELECT");
    assert!(parameters::enrich(&mut job, CHECKBOX_FORM).unwrap_err().contains("Unsupported parameter"));
}

#[tokio::test]
async fn native_build_preserves_array_and_scalar_types_and_queue() {
    for values in [json!([]), json!(["gateway"]), json!(["gateway", "支付&清算"])] {
        let (base, task) = server(vec![
            response("200 OK", "", &checkbox_job().to_string()), response("200 OK", "", CHECKBOX_FORM),
            response("201 Created", "Location: /jenkins/queue/item/25/\r\n", ""),
        ]).await;
        let req = JenkinsRequest { path: vec!["deploy".into()], parameter_revision: Some(checkbox_revision()),
            parameters: HashMap::from([("MODULES".into(), values.clone())]), ..Default::default() };
        assert_eq!(client(&base).trigger(&req).await.unwrap()["queueId"], 25);
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[1].starts_with("GET /jenkins/job/deploy/build "));
        assert!(requests[1].to_ascii_lowercase().contains("authorization: basic "));
        assert!(requests[2].starts_with("POST /jenkins/job/deploy/build "));
        let body = requests[2].split("\r\n\r\n").nth(1).unwrap();
        let encoded = Url::parse(&format!("http://fixture.invalid/?{body}")).unwrap();
        let form: HashMap<_, _> = encoded.query_pairs().into_owned().collect();
        let payload: Value = serde_json::from_str(&form["json"]).unwrap();
        assert_eq!(payload["parameter"][1]["value"], values);
        assert_eq!(payload["parameter"][0]["value"], "stage");
        assert_eq!(payload["parameter"][2]["value"], "test");
        assert_eq!(payload["parameter"][3]["value"], true);
        assert_eq!(form["statusCode"], "201");
    }
}

#[tokio::test]
async fn native_build_errors_never_post() {
    for (html, status, revision, values) in [
        (CHECKBOX_FORM.to_owned(), "200 OK", checkbox_revision(), json!(["unknown"])),
        (CHECKBOX_FORM.to_owned(), "200 OK", checkbox_revision(), json!(["gateway", "gateway"])),
        (CHECKBOX_FORM.to_owned(), "200 OK", checkbox_revision(), json!("gateway")),
        (CHECKBOX_FORM.replace("gateway", "new-module"), "200 OK", checkbox_revision(), json!([])),
        (CHECKBOX_FORM.to_owned(), "200 OK", "old".into(), json!([])),
        ("<html>Login</html>".into(), "200 OK", checkbox_revision(), json!([])),
        (String::new(), "403 Forbidden", checkbox_revision(), json!([])),
    ] {
        let (base, task) = server(vec![response("200 OK", "", &checkbox_job().to_string()), response(status, "", &html)]).await;
        let req = JenkinsRequest { path: vec!["deploy".into()], parameter_revision: Some(revision),
            parameters: HashMap::from([("MODULES".into(), values)]), ..Default::default() };
        assert!(client(&base).trigger(&req).await.is_err());
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request.starts_with("GET ")));
    }
}

#[tokio::test]
async fn native_build_does_not_retry_ambiguous_responses() {
    for (status, headers) in [("200 OK", ""), ("302 Found", "Location: /jenkins/\r\n"), ("503 Unavailable", "")] {
        let (base, task) = server(vec![response("200 OK", "", &checkbox_job().to_string()),
            response("200 OK", "", CHECKBOX_FORM), response(status, headers, "")]).await;
        let request = JenkinsRequest { path: vec!["deploy".into()], parameter_revision: Some(checkbox_revision()), ..Default::default() };
        assert!(client(&base).trigger(&request).await.unwrap_err().contains("JENKINS_UNCONFIRMED"));
        assert_eq!(task.await.unwrap().iter().filter(|request| request.starts_with("POST ")).count(), 1);
    }
}

fn client(base: &str) -> JenkinsClient {
    JenkinsClient {
        http: Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap(),
        base: Url::parse(base).unwrap(),
        display_base: Url::parse(base).unwrap(),
        username: "builder".into(),
        token: "secret-token".into(),
    }
}

fn response(status: &str, headers: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len())
}

async fn server(responses: Vec<String>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/jenkins/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(4), listener.accept()).await.unwrap().unwrap();
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
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
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

#[test]
fn url_validation_and_segment_encoding() {
    for url in
        ["file:///tmp/jenkins", "https://user:token@example.com/", "https://ci/?token=secret", "https://ci/#fragment"]
    {
        assert!(JenkinsConfig { server_addr: url.into(), tls_skip_verify: false }.base_url().is_err());
    }
    let c = client("https://ci.example.com/jenkins/");
    let url = c.endpoint(&["team space".into(), "feature/a".into()], &["api", "json"]).unwrap();
    assert_eq!(url.as_str(), "https://ci.example.com/jenkins/job/team%20space/job/feature%2Fa/api/json");
    assert!(c.endpoint(&["..".into()], &["api", "json"]).is_err());
    assert!(!c.safe_redirect("https://other.example/jenkins/"));
    assert!(!c.safe_redirect("/jenkins/login"));
    assert!(!c.safe_redirect("/outside"));
}

#[test]
fn queue_location_uses_only_valid_numeric_id() {
    assert_eq!(queue_id_from_location("http://internal:8080/jenkins/queue/item/42/"), Some(42));
    assert_eq!(queue_id_from_location("/jenkins/queue/item/42/"), Some(42));
    for location in ["/queue/item/0", "/queue/item/abc/", "/queue/item/42/evil", "/login"] {
        assert_eq!(queue_id_from_location(location), None);
    }
}

#[tokio::test]
async fn probe_authenticates_and_rejects_html() {
    let (base, task) = server(vec![response("200 OK", "X-Jenkins: 2.555\r\n", r#"{"jobs":[],"mode":"NORMAL"}"#)]).await;
    let result = client(&base).probe().await.unwrap();
    assert_eq!(result["version"], "2.555");
    let requests = task.await.unwrap();
    assert!(requests[0].starts_with("GET /jenkins/api/json?tree="));
    assert!(requests[0].to_lowercase().contains("authorization: basic "));
    assert!(!requests[0].lines().next().unwrap().contains("secret-token"));
    let (base, task) = server(vec![response("200 OK", "Content-Type: text/html\r\n", "<html>Login</html>")]).await;
    assert!(client(&base).probe().await.unwrap_err().contains("non-JSON"));
    task.await.unwrap();
}

#[tokio::test]
async fn triggers_standard_parameters_and_maps_queue_without_following_location() {
    let job = r#"{"buildable":true,"property":[{"parameterDefinitions":[{"name":"TARGET","_class":"hudson.model.ChoiceParameterDefinition","choices":["dev","prod"]},{"name":"FLAG","_class":"hudson.model.BooleanParameterDefinition"}]}]}"#;
    let (base, task) = server(vec![
        response("200 OK", "", job),
        response("201 Created", "Location: http://internal/queue/item/9/\r\n", ""),
    ])
    .await;
    let req = JenkinsRequest {
        path: vec!["folder".into(), "deploy".into()],
        parameters: HashMap::from([("TARGET".into(), json!("dev")), ("FLAG".into(), json!(false))]),
        ..Default::default()
    };
    let result = client(&base).trigger(&req).await.unwrap();
    assert_eq!(result["queueId"], 9);
    let requests = task.await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("POST /jenkins/job/folder/job/deploy/buildWithParameters "));
    assert!(requests[1].contains("TARGET=dev&FLAG=false"));
}

#[tokio::test]
async fn unsupported_parameters_never_send_a_build_post() {
    let (base, task) = server(vec![response("200 OK", "", r#"{"buildable":true,"property":[{"parameterDefinitions":[{"name":"FILE","_class":"hudson.model.FileParameterDefinition"}]}]}"#)]).await;
    let error =
        client(&base).trigger(&JenkinsRequest { path: vec!["job".into()], ..Default::default() }).await.unwrap_err();
    assert!(error.contains("Unsupported parameter"));
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn large_log_replays_incomplete_utf8_and_advances_only_consumed_bytes() {
    let body = format!("{}构建", "a".repeat(LOG_LIMIT - 1));
    let (base, task) =
        server(vec![response("200 OK", &format!("X-Text-Size: {}\r\nX-More-Data: false\r\n", body.len()), &body)])
            .await;
    let req = JenkinsRequest { path: vec!["job".into()], number: Some(1), ..Default::default() };
    let chunk = client(&base).log(&req).await.unwrap();
    assert_eq!(chunk.next_start, (LOG_LIMIT - 1) as u64);
    assert!(!chunk.text.contains('�'));
    assert!(chunk.more);
    assert!(task.await.unwrap()[0].contains("progressiveText?start=0"));
}

#[tokio::test]
async fn write_failures_are_not_retried_and_tokens_are_not_returned_in_errors() {
    let (base, task) = server(vec![response("503 Service Unavailable", "", "secret-token")]).await;
    let c = client(&base);
    let error = c.send(Method::POST, c.endpoint(&[], &["build"]).unwrap(), Some(&[])).await.unwrap_err();
    assert!(error.contains("JENKINS_UNCONFIRMED"));
    assert!(!error.contains("secret-token"));
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn permission_and_redirect_errors_do_not_fall_back_to_other_endpoints() {
    for (status, headers, expected) in [
        ("401 Unauthorized", "", "JENKINS_AUTH"),
        ("403 Forbidden", "", "JENKINS_FORBIDDEN"),
        ("404 Not Found", "", "JENKINS_NOT_FOUND"),
        ("302 Found", "Location: https://other.example/login\r\n", "JENKINS_REDIRECT"),
    ] {
        let (base, task) = server(vec![response(status, headers, "")]).await;
        assert!(client(&base).probe().await.unwrap_err().contains(expected));
        assert_eq!(task.await.unwrap().len(), 1);
    }
}
