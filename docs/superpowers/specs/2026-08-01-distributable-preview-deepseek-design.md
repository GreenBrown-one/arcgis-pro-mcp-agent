# ArcGIS Pro 智能助手可分发预览版设计规格

- 日期：2026-08-01
- 状态：设计已获用户确认，待书面规格复核
- 版本目标：`0.2.0-preview.1`
- 基线提交：`07f8dfb6be0c865e933aa78d1b06de6cb54e4636`
- 实施分支：`feature/distributable-preview-deepseek`
- 目标环境：Windows 10/11 x64、ArcGIS Pro 3.7

## 1. 背景

当前 `0.1.0` 已完成并实机验证 Tauri + React 桌面端、Codex App Server、.NET 8 MCP Server、本地 Bridge、.NET 10 ArcGIS Pro Add-In 以及 17 个 R0/R1 工具。现有版本仍是开发安装形态：桌面 bundle 未启用，启动入口位于开发版本目录，ArcGIS Pro 安装解析只存在于 PowerShell 开发脚本，模型入口只支持本机已有的 Codex 命令和 ChatGPT 账号登录。

本阶段不是正式商业发布，而是一个可以发给试用者安装、了解产品并反馈意见的预览版。它必须容易安装和打开，允许用户在 DeepSeek API 与 ChatGPT 订阅登录之间二选一，并自动发现本机 ArcGIS Pro。所有新增能力必须与已经验证的 ArcGIS 核心隔离，便于后续删除、替换或重做。

## 2. 目标

预览版完成后，未安装开发工具的试用者可以：

1. 运行一个 Windows 安装程序完成当前用户安装；
2. 从桌面或开始菜单打开“ArcGIS Pro 智能助手”；
3. 让应用自动发现 ArcGIS Pro 3.7，或在自动发现失败时手动定位；
4. 在 DeepSeek API Key 与 ChatGPT 账号登录之间二选一，并可稍后切换；
5. 一键启动 ArcGIS Pro，自动建立 Add-In、Bridge、MCP 与模型提供商连接；
6. 使用现有 17 个 R0/R1 工具完成工程、地图、图层、查询、临时选择和导航等基础操作；
7. 在连接异常时看到分层原因并执行重检、修复或重连；
8. 导出不含凭据和地图数据的诊断信息，便于手动反馈；
9. 使用标准 Windows 卸载入口移除本软件，而不损坏 ArcGIS Pro 或用户 GIS 数据。

## 3. 非目标

本阶段明确不做：

- 自动模型路由、负载均衡或 DeepSeek/Codex 自动故障切换；
- 同时使用两个模型提供商处理同一轮对话；
- 多用户、组织管理、许可证服务器、计费或支付；
- 自动更新、插件市场、云同步、遥测后台或自动上传反馈；
- 新增 R2/R3 写入、保存、导出、地理处理、数据编辑或删除工具；
- 修改现有 17 个 R0/R1 工具的业务语义；
- 支持 ArcGIS Desktop 10.x、ArcMap 或 ArcGIS Pro 3.7 以外版本的正式兼容承诺；
- 正式付费销售、Microsoft Store 上架或正式代码签名发布。

## 4. 源码和发布隔离

所有预览版工作只发生在 `feature/distributable-preview-deepseek` 分支及其独立 worktree：

`E:\Ai_Project\codex\MCP-Server-ArcGIS-Pro-AddIn\.worktrees\distributable-preview-deepseek`

该分支从已验收的 `07f8dfb` 创建。`master` 和 `feature/arcgis-pro-agent-foundation` 不接收预览版代码、依赖或打包提交。未经用户明确批准，不合并、不变基、不覆盖、不推送，也不创建公开 GitHub Release。

预览版使用独立版本号 `0.2.0-preview.1`。安装目录、配置目录、日志和安装所有权清单都携带独立产品/版本标识，从而允许与开发版分辨、回滚或删除。

## 5. 总体架构

### 5.1 稳定 ArcGIS 核心

以下组件保持模型无关：

- ArcGIS Pro Add-In；
- .NET MCP Server；
- 本地命名管道 Bridge；
- ArcGIS Pro 安装发现与进程启动；
- Add-In 安装、版本检查和修复；
- ArcGIS 连接状态机；
- 现有 17 个 R0/R1 工具及其风险边界。

模型提供商不得进入 Add-In、Bridge 或 MCP 工具的参数和响应协议。

### 5.2 可替换提供商层

桌面后端增加源码级可插拔的提供商边界：

- `ProviderContract`：定义身份状态、会话创建、发送一轮、取消、健康检查和统一事件；
- `ProviderRegistry`：根据设置选择且仅启动一个提供商；
- `CodexProvider`：封装 ChatGPT 账号登录和 Codex App Server；
- `DeepSeekProvider`：封装 DeepSeek API、对话状态和 MCP 工具调用循环；
- `ProviderEventMapper`：把提供商专用事件归一化为前端事件。

前端只消费统一状态和事件，不判断 Codex JSON-RPC 方法名，也不解释 DeepSeek API 响应。该边界是源码级模块化，不在预览版中引入动态 DLL、第三方插件加载或远程插件市场。

### 5.3 统一提供商契约

提供商至少支持以下能力：

- `get_auth_status`：返回未配置、需要登录、就绪或失败等安全分类；
- `configure`：保存非秘密设置，并通过专用秘密存储写入凭据；
- `start_session`：创建提供商会话并返回应用级会话标识；
- `send_turn`：发送用户消息并流式产生统一事件；
- `cancel_turn`：取消当前轮，不关闭 ArcGIS 核心；
- `health_check`：验证提供商运行时或远端服务；
- `sign_out`：只清除该应用所属的提供商身份数据。

统一事件仅包含应用需要的白名单字段，例如文本增量、工具开始、工具完成、安全错误分类和一轮完成。原始远端错误、访问令牌、请求头或完整内部响应不得直达前端和日志。

## 6. Codex / ChatGPT 订阅适配器

Codex 路径沿用已经实机验证的 App Server 协议和事件状态机。预览版固定一个已测试的 Windows x64 Codex CLI 版本，以 Tauri sidecar 或等价的应用私有运行时方式随版本交付；最终打包保留 Apache-2.0 许可证和所需第三方声明。

Codex 运行时使用应用专用的 `CODEX_HOME`，位于当前用户的 ArcGIS Pro 智能助手数据目录，不读取或覆盖用户其他 Codex 安装的配置和身份状态。该环境变量只注入本应用创建的 Codex 子进程，不写入系统或用户全局环境。登录按钮启动官方 ChatGPT 浏览器登录流程；用户不输入所谓“订阅密钥”，也不得复制、导出或共享会话令牌。

选择 Codex 时：

1. 检查应用私有 Codex 运行时及其版本；
2. 启动 `app-server --stdio`；
3. 查询身份状态，必要时启动浏览器登录；
4. 创建会话并配置现有 ArcGIS MCP Server；
5. 将 App Server 事件映射为统一提供商事件；
6. 在退出登录时只清理应用专用 Codex 身份目录。

Codex CLI 的开源许可与 OpenAI 在线服务条款是两个边界。每名试用者必须使用自己的 ChatGPT 账号并受其账号方案、使用限额和适用条款约束。正式付费发布前必须再次执行第三方许可和服务条款审查。

## 7. DeepSeek 适配器

DeepSeek 适配器直接调用官方 OpenAI 兼容接口，默认基地址为 `https://api.deepseek.com`。默认模型由提供商配置提供，首发默认值为 `deepseek-v4-flash`；模型标识不得散落在前端、MCP 或 ArcGIS Add-In 中，以便后续更换。

DeepSeek API 是无状态多轮接口，适配器在应用会话内维护用户、助手和工具消息历史。每次请求传递当前会话所需历史；预览版不新增云端会话同步。

工具循环按以下顺序执行：

1. 启动或复用应用私有的 MCP Server；
2. 完成 MCP 初始化并读取工具清单；
3. 只暴露与基线匹配的 17 个 R0/R1 工具；
4. 将工具定义传给 DeepSeek；
5. 对模型返回的工具调用执行名称白名单和 JSON 参数验证；
6. 通过 MCP `tools/call` 执行并把脱敏结果加入本轮历史；
7. 继续模型调用，直到得到最终文本、用户取消、超时或达到工具循环上限。

单轮设置明确的超时、最大工具轮数和最大上下文预算。401/403 不重试并提示重新配置 Key；429 和可恢复的 5xx 使用有界退避；参数错误、未知工具和协议错误失败关闭，不执行猜测性工具调用。

## 8. 凭据、配置和本地数据

DeepSeek API Key 存入 Windows Credential Manager，使用应用专用服务名。Key 不得进入前端持久状态、JSON 配置、环境文件、命令行、崩溃信息或日志。界面只显示“已配置”和经过掩码处理的末尾少量字符；读取秘密只发生在 Tauri 后端发起远端请求时。

普通设置可以写入应用数据目录，包括：

- 当前提供商；
- DeepSeek 基地址和模型标识；
- 已确认的 ArcGIS Pro 安装路径；
- 首次启动步骤完成状态；
- 应用、Add-In、MCP 与 Bridge 版本。

ChatGPT 身份数据只由应用私有 Codex 运行时管理。卸载时允许清理本应用专用 Codex 目录，但不得删除用户其他 Codex、ChatGPT 或 OpenAI 软件的数据。

## 9. Windows 安装包

Tauri bundle 从开发模式切换为生产 NSIS 单文件安装程序，默认执行当前用户安装，不要求用户具备 Visual Studio、Node.js、Rust 或 .NET SDK。安装包至少携带：

- Tauri 桌面程序和 WebView 资源；
- 自包含发布的 .NET 8 MCP Server 和 Bridge；
- ArcGIS Pro 3.7 / .NET 10 Add-In 包；
- 固定版本的 Windows x64 Codex 运行时及许可证声明；
- 版本、文件哈希和安装所有权清单；
- 用于修复和卸载的必要资源。

安装器创建桌面快捷方式、开始菜单入口和 Windows 卸载项。快捷方式直接启动已安装桌面程序，最终用户不需要知道 `%LOCALAPPDATA%` 下的真实可执行文件路径。

正式商业发布所需的代码签名证书、发布者名称和自动更新基础设施不属于本预览版。未签名预览安装包必须明确显示版本和 SHA-256，且只向已知试用者分发。

## 10. ArcGIS Pro 自动发现与 Add-In 管理

产品运行时在首次启动和用户点击“重新检测”时执行 ArcGIS Pro 发现。发现优先级为：

1. 用户已经确认且仍通过验证的绝对路径；
2. 64 位 Windows 注册表中的 Esri ArcGIS Pro 安装信息；
3. ArcGIS Pro 标准安装目录；
4. 当前环境兼容候选目录，包括 `D:\arcgis_pro`；
5. 用户通过文件选择器指定的 `ArcGISPro.exe`。

候选目录必须经过规范化并同时验证 `bin\ArcGISPro.exe`、`bin\ArcGIS.Core.dll` 等必需文件。只匹配目录名称不算成功。发现多个有效版本时，优先选择兼容的 3.7，并在界面显示实际路径和版本；未知或不兼容版本不静默使用。

Add-In 安装沿用已经测试的每用户目录：

`%USERPROFILE%\Documents\ArcGIS\AddIns\ArcGISPro\{1A0481EA-3F43-4C98-B4B5-A58C727CD115}`

安装、升级、修复和卸载只管理安装所有权清单记录的 `ArcGISProAgent.AddIn.esriAddinX`。不得递归删除整个 ArcGIS AddIns 目录，也不得碰触其他厂商 Add-In。

如果 ArcGIS Pro 正在运行且 Add-In 需要安装、升级或修复，应用只提示用户保存工作并确认重启；未经用户确认不得结束 ArcGIS Pro 进程。Add-In 文件替换完成后必须重新启动 ArcGIS Pro，不能把旧进程中的旧 Add-In 误报为当前版本。

## 11. 首次启动和日常启动

首次启动向导保持最短路径：

1. 检测 ArcGIS Pro；
2. 自动安装或修复当前版本 Add-In；
3. 选择 DeepSeek API 或 ChatGPT 账号；
4. 完成 Key 配置或浏览器登录；
5. 执行本地组件和提供商自检；
6. 显示“启动 ArcGIS Pro 并连接”。

日常启动恢复上次选择的提供商并立即进行健康检查。用户点击主按钮后，应用使用已验证的绝对路径直接启动 `ArcGISPro.exe`，随后等待 Add-In 握手、Bridge 心跳、MCP 初始化和工具基线验证。只有四层均通过时才显示“ArcGIS 已连接”。

源代码使用者另有明确入口：仓库根目录提供一个轻量的 `Open-Project.ps1`，只负责定位并打开 `McpServer.sln`；开发文档同时给出 Visual Studio 和命令行入口。该脚本不进入最终用户的安装包。

## 12. 连接状态和恢复

界面不再用单一的“连接已过期”覆盖所有故障，而是分别维护：

- ArcGIS Pro 安装发现状态；
- ArcGIS Pro 进程状态；
- Add-In 安装和协议版本；
- Bridge 心跳与连接租约；
- MCP 进程、初始化和工具基线；
- 当前模型提供商身份与健康状态。

Bridge 心跳暂时中断时进入“正在重连”，在有限窗口内自动恢复；超过租约后标记为未连接，但保留“重新连接”和“重新检测”操作。重新连接不得要求用户重新安装整个软件，也不得把上一运行时世代的事件误认为当前就绪。

错误必须映射为稳定、安全、可操作的类别，例如：`arcgis_not_found`、`addin_missing`、`bridge_reconnecting`、`mcp_unavailable`、`provider_auth_required`、`provider_rate_limited`。原始远端响应和绝对用户数据路径不直接显示或写入普通日志。

## 13. 诊断、隐私和反馈

预览版不建立遥测服务，不自动上传日志、提示词、地图上下文、工具结果或设备标识。提供两个本地操作：

- “复制诊断信息”：复制版本、组件状态、安全错误分类和经过缩短的非敏感路径摘要；
- “导出脱敏日志”：生成用户主动选择并手动发送的诊断包。

脱敏层必须过滤 API Key、Authorization 头、Codex 登录数据、电子邮件、完整提示词、完整地图属性值、数据库连接字符串和任意已知用户目录。诊断包生成前显示内容范围，并允许用户取消。

## 14. 卸载、删除和后续替换

标准卸载移除本软件的安装目录、快捷方式、开始菜单项、卸载注册项、安装所有权清单记录的 Add-In、应用私有 MCP/Bridge/Codex 运行时、普通配置和 DeepSeek Credential Manager 条目。它不得删除：

- ArcGIS Pro；
- `.aprx`、地图、图层、文件地理数据库或其他 GIS 数据；
- 其他 ArcGIS Add-In；
- 用户独立安装的 Codex CLI、Codex 桌面应用或其数据。

提供商适配器保持独立目录、独立测试和单一注册点。删除 DeepSeek 时只移除 DeepSeek 模块、凭据设置入口和相关测试；删除 Codex 时只移除 Codex 模块、sidecar 和登录入口。两种删除都不得修改 Add-In、Bridge、MCP 工具协议或 ArcGIS 发现逻辑。

## 15. 测试策略

实现遵循测试驱动开发，先写失败测试，再做最小实现。测试分层如下。

### 15.1 Rust / Tauri 单元测试

- 提供商注册表只激活一个提供商；
- 两种提供商事件归一化到同一白名单模型；
- DeepSeek Key 永不出现在序列化设置和日志；
- ArcGIS 候选路径优先级、规范化和文件验证；
- 多版本发现优先选择兼容的 3.7；
- 连接租约过期、自动重连、世代隔离和显式重试；
- 卸载所有权范围不包含 ArcGIS 工程或其他 Add-In。

### 15.2 DeepSeek 合约和集成测试

使用本地伪造 HTTP 服务和测试 MCP Server 覆盖：

- Key 验证成功与 401/403；
- 流式文本；
- 单个和连续多个工具调用；
- 未知工具、无效参数、超时、取消和工具循环上限；
- 429/5xx 有界退避；
- 多轮历史拼接；
- 工具和错误内容脱敏。

自动化测试不依赖真实 DeepSeek Key。真实 Key 只用于最终人工烟雾测试，并通过 Windows Credential Manager 输入。

### 15.3 Codex 回归测试

- 应用私有 Codex runtime 解析；
- 未登录、浏览器登录、已登录和退出登录；
- 当前和兼容状态事件仍进入单一状态机；
- 现有 ChatGPT 订阅会话、流式消息和 17 工具能力不回归；
- 应用私有 `CODEX_HOME` 不读写用户其他 Codex 安装。

### 15.4 安装和卸载测试

- NSIS 全新安装、覆盖升级、修复和卸载；
- 桌面与开始菜单快捷方式可启动；
- 路径包含空格和非 ASCII 用户名；
- Add-In 安装到正确 GUID 目录；
- 文件哈希和安装所有权清单一致；
- 卸载后 ArcGIS Pro、项目和其他 Add-In 保持不变；
- 开发机和一台不含 SDK/Node/Rust 的 Windows 测试环境均可运行。

### 15.5 ArcGIS Pro 3.7 实机验收

1. 安装预览包并从桌面打开；
2. 自动识别真实 ArcGIS Pro 3.7；
3. 分别完成 DeepSeek 与 ChatGPT 两条提供商路径；
4. 一键启动 ArcGIS Pro 并建立 Add-In、Bridge、MCP 连接；
5. 在不保存项目、不写入源数据的前提下执行健康检查、读取工程、列出地图和图层；
6. 人为重启 Bridge 或 ArcGIS Pro，验证重连和错误提示；
7. 导出脱敏诊断信息并检查不存在秘密；
8. 卸载并核实 GIS 数据未受影响。

## 16. 交付物

本阶段交付：

- `ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe`；
- 安装包 SHA-256；
- 安装包内第三方许可证和 NOTICE；
- 预览版用户操作手册；
- 开发者打开、构建和修改说明；
- 脱敏诊断导出说明；
- 自动化测试结果和 ArcGIS Pro 3.7 实机验收记录；
- 已知限制和反馈指南。

## 17. 完成条件

只有同时满足以下条件，预览版才算完成：

- 原主分支和现有 foundation worktree 没有预览版代码提交；
- 两个模型提供商二选一并可在设置中切换；
- DeepSeek Key 只存于 Windows Credential Manager；
- ChatGPT 登录使用应用私有官方 Codex 运行时；
- ArcGIS Pro 3.7 自动发现、手动回退和一键启动可用；
- Add-In、Bridge、MCP 与提供商具有分层、可恢复状态；
- 现有 17 个 R0/R1 工具无语义扩展且回归通过；
- NSIS 安装、快捷方式、覆盖升级和标准卸载通过；
- 诊断输出通过秘密和隐私扫描；
- 全量自动化测试与两条提供商实机烟雾测试通过；
- 交付安装包、SHA-256、许可证声明和操作手册。

## 18. 回滚边界

预览版的提供商、生产安装和发现逻辑必须形成独立提交序列。若 DeepSeek 适配器失败，可回退该适配器及其 UI 入口，同时保留 Codex、ArcGIS 核心和安装器；若生产安装器失败，可回退 bundle 配置和安装资源，同时保留提供商代码；若 ArcGIS 发现回归，可回退运行时发现模块并继续使用显式路径。任何回滚都不得回退已验收的 Add-In、MCP/Bridge 协议和现有 17 个工具实现。

## 19. 参考依据

- OpenAI Codex SDK：<https://developers.openai.com/codex/sdk/>
- OpenAI Codex CLI：<https://github.com/openai/codex>
- 使用 ChatGPT 方案登录 Codex：<https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan>
- DeepSeek API 快速开始：<https://api-docs.deepseek.com/zh-cn/>
- DeepSeek 多轮对话：<https://api-docs.deepseek.com/guides/multi_round_chat/>
