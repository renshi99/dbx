# Jenkins 管理连接

DBX 桌面版与 Web 版通过共用 Rust HTTP 客户端访问 Jenkins 2.x Remote Access API。无需安装额外 Jenkins 插件。

## 建立连接

在连接选择器中选择 Jenkins，填写完整服务地址，例如 `https://ci.example.com/jenkins`。地址保留反向代理前缀，不能包含账号、查询参数或片段。填写用户名和该用户的 API Token；用户名留空为匿名只读模式。Token 使用项目现有凭据保存机制；关闭保存密码后仅保留当前会话凭据。TLS 默认验证证书。支持已有连接传输层。

测试连接需要读取控制器的 `/api/json`。任务可见范围和实际写权限由 Jenkins 账户权限决定；DBX 的只读锁还会阻止构建、取消和停止操作。

通过隧道访问 HTTPS 时，应填写证书对应的主机名，以保留 TLS 身份校验。使用 IP 地址且启用证书校验的 HTTPS 隧道会返回明确错误。

## 首版功能

- 在当前目录筛选任务，进入 Folder、Organization Folder 和多分支 Pipeline 容器。
- 查看 Freestyle / Pipeline 任务详情和每页 50 条构建历史。
- 触发普通或参数化构建，跟踪排队到实际构建，取消排队、请求停止构建。
- 参数支持字符串、文本、布尔、固定选项，以及 Extended Choice Parameter 插件的 `PT_CHECKBOX` 多选。遇到其他参数类型时在 Jenkins 原页面操作。
- 按字节偏移每 2 秒增量读取控制台日志，每次最多 1 MiB，界面保留最近 5 MiB。页面隐藏时暂停轮询，关闭工作区后停止。日志只作为文本显示。

写操作不会自动重试。请求超时或服务端异常后，应刷新队列与历史确认结果，再决定是否重新提交。收到停止/取消响应仅表示已请求，最终状态以 Jenkins 返回为准。队列项过期时尝试用最近构建的 `queueId` 对应；无法对应则显示状态未知。

不支持密码登录、SSO、其他插件参数、任务配置编辑、节点或凭据管理、产物下载和阶段视图。

## Extended Choice 多选构建

包含 `com.cwctravel.hudson.plugins.extended_choice_parameter.ExtendedChoiceParameterDefinition` 且类型为 `PT_CHECKBOX` 的任务，会使用现有连接认证读取原生构建表单。选项和默认值从静态 HTML 解析，不执行脚本，不读取 `config.xml`，不需要 `Job/Configure` 权限。表单依赖脚本生成或无法解析时禁用构建，并保留日志查看。

任务详情增加 `parameterRevision` 和可选的 `parameterError`；多选参数使用 `choices` 与数组形式的 `defaultParameterValue.value`。构建请求的 `parameters` 支持字符串、布尔和字符串数组，插件参数任务需回传 `parameterRevision`。后端提交前重新读取定义及表单，参数发生变化时要求刷新任务。

插件任务向 `build` 提交原生 `json.parameter`，多选值保持数组，由 Jenkins 插件处理分隔符。仅 HTTP 201 视为明确受理；不明确的响应不会自动重试。基础参数任务继续使用 `buildWithParameters`。认证与 CSRF 行为沿用现有 API Token 方式。

## 实现与验证

连接类型描述符是注册信息源；更新描述符后使用 `pnpm generate:connection-types` 生成派生文件。Rust `jenkins` 模块使用连接池保存客户端，共享配置变更失效、断开和临时凭据生命周期。

前端导出类型化 `jenkins*` 操作；Tauri 使用 `jenkins_request`，Web 使用 `POST /api/jenkins/request`，载荷为 `{ operation, request }`。Rust 核心维护操作白名单，所有目标都从已保存连接和任务路径分段构建，不接受任意目标 URL。

```sh
pnpm check:connection-types
pnpm typecheck
pnpm test apps/desktop/src/lib/jenkins/jenkins.spec.ts apps/desktop/src/components/jenkins/JenkinsWorkspace.spec.ts
cargo test -p dbx-core --no-default-features --features sqlite-bundled jenkins
cargo check -p dbx-web
cargo check -p dbx
```

真实 Jenkins 联调应使用独立测试任务，依次验证文件夹导航、标准参数、队列转构建、日志完成、取消排队和停止构建。自动化单元测试使用本地 HTTP 模拟服务，不会操作实际 Jenkins。
