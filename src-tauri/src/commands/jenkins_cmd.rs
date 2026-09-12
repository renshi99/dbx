use dbx_core::connection::AppState;
use dbx_core::jenkins::JenkinsRequest;

#[tauri::command]
pub async fn jenkins_request(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    operation: String,
    request: JenkinsRequest,
) -> Result<serde_json::Value, String> {
    dbx_core::jenkins::execute(&state, &operation, request).await
}
