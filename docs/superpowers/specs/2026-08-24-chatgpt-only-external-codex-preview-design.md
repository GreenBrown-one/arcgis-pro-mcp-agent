# ChatGPT-only 外部 Codex 预览版规格修订

**日期：** 2026-08-24  
**状态：** 已确认，可进入实施
**实施分支：** `feature/distributable-preview-deepseek`  
**实施 worktree：** `.worktrees/distributable-preview-deepseek`

## 1. 修订目的

首个可分发预览版只支持 ChatGPT 订阅登录。DeepSeek API、工具循环、API Key 输入和模型提供商切换全部延期。本修订覆盖 `2026-08-01-distributable-preview-deepseek-design.md` 中关于首发双提供商、首次启动和打包 Codex CLI 的要求；已完成并验收的 Task 1–4 不回退，旧规格保留为历史记录。

本阶段优先级是：安装方便、启动方便、能够交给试用者完成最基础的 ArcGIS Pro 操作并反馈意见。不得为了未来功能扩张当前预览版。

## 2. 已确认的产品决策

1. 首发只有 ChatGPT，不显示 DeepSeek 或提供商选择。
2. 采用 B1 外部依赖方案：安装包不携带 Codex CLI；自动检测用户已经安装的 Codex CLI。
3. 找不到 Codex 时，只提供官方安装说明和重新检测，不自动安装、升级或修改用户环境。
4. Codex 版本与验收版本不同时先警告，再进行实际兼容性自检；自检通过即可使用。
5. 使用应用私有 `CODEX_HOME`。即使系统 Codex 已登录，用户仍需为本软件完成一次独立的 ChatGPT 浏览器登录。
6. ArcGIS Pro 继续自动发现，并保留手动选择 `ArcGISPro.exe` 的回退。
7. Add-In 继续使用 Esri 官方文件关联或安装工具打开固定的 `.esriAddInX`，桌面端不直接写入或递归删除 ArcGIS AddIns。

## 3. 首发范围

### 3.1 包含

- Windows 10/11 x64 当前用户 NSIS 安装包；
- 桌面和开始菜单快捷方式；
- ArcGIS Pro 3.7 自动发现、验证、手动回退和一键启动；
- 固定的 ArcGIS Add-In 安装入口与重启指引；
- 应用私有 MCP Server、Bridge 运行链路和现有 17 个 R0/R1 工具；
- 外部 Codex CLI 自动发现、版本检查、兼容性自检；
- 应用私有 ChatGPT 登录状态和现有 Codex App Server 会话；
- 单页首次使用检查、分层错误提示、重新检测和重新连接；
- 安装包 SHA-256、第三方声明和简明用户手册；
- Windows 标准卸载入口及精确的应用所有权清理。

### 3.2 不包含

- DeepSeek HTTP/API、模型或工具循环；
- DeepSeek API Key 输入、设置或前端命令；
- 模型提供商选择、切换、自动路由或故障转移；
- 自动安装或自动升级 Codex CLI；
- 依赖 Codex 桌面应用内部的 WindowsApps 私有文件；
- 自动更新、遥测后台、账号共享、计费或正式商业许可系统；
- 新增 ArcGIS 工具、保存、导出、地理处理或源数据编辑。

## 4. 运行架构

安装包包含桌面应用、应用私有 MCP Server、Add-In 包及运行所需资源，但不包含 `codex.exe`、`codex.cmd` 或 Codex npm 包。ChatGPT 路径仍使用已验收的 Codex App Server 协议；MCP、Bridge、Add-In 和 17 个工具语义保持不变。

启动时桌面后端并行执行三组检查：

1. 外部 Codex CLI 发现与兼容性自检；
2. ArcGIS Pro 3.7 发现与路径验证；
3. 应用私有资源、Add-In 和本地连接状态检查。

所有检查都返回结构化、安全状态。前端不解析 Codex JSON-RPC、MCP 原始帧或后端异常文本。

## 5. 外部 Codex CLI 发现

### 5.1 候选来源

检测器仅检查稳定、可解释的用户级来源：

1. 当前进程 `PATH` 中的 `codex.exe` 或 `codex.cmd`；
2. 当前用户 npm 全局命令目录中的 `codex.cmd`。

检测器不得扫描整个磁盘，不得调用 Codex 桌面应用包内的私有可执行文件，也不得接受来自前端的任意命令、参数或未经验证路径。候选必须解析为绝对路径并通过普通文件检查；执行时使用验证后的精确路径。

### 5.2 有界验证

对候选执行有界的 `codex --version`：

- 限制执行时间和 stdout/stderr 大小；
- 只解析版本，不把原始输出直接显示或写入普通日志；
- 无法启动、超时、输出畸形或退出失败时尝试下一个候选；
- 所有候选失败后返回稳定的 `codex_not_found` 或 `codex_invalid` 状态。

本修订的起始验收版本为本机已检测的 `codex-cli 0.149.0`，并记录在后端单一常量中，不散落在 UI、脚本和测试中。后续版本不同返回 `codex_version_unverified` 警告，但不立即阻止用户；不得在没有兼容性测试记录时静默改写验收版本。

### 5.3 兼容性自检

版本验证后，用应用私有 `CODEX_HOME` 启动 Codex App Server，并执行已有初始化与账户状态读取。自检必须有总超时、输出上限和子进程清理：

- 协议兼容且未登录：进入“需要登录 ChatGPT”；
- 协议兼容且已登录：进入“Codex 已就绪”；
- 协议不兼容或异常退出：显示可操作的更新/重装指引，不进入对话页。

版本不同但兼容性自检通过时允许继续，并保留非阻断警告。

## 6. 登录与数据隔离

Codex 子进程只获得本软件的私有 `CODEX_HOME`，不得读取或覆盖用户全局 Codex 配置、Codex 桌面应用状态或其他产品的登录数据。

首次登录调用已有官方 ChatGPT 浏览器登录流程。用户不输入“订阅 Key”，应用不得复制、导出或展示认证令牌。登录完成后重新读取账户状态；退出登录只影响本软件私有身份。

ChatGPT-only 运行时的有效提供商固定为 Codex。旧开发设置中的 DeepSeek 选择不得使应用进入 DeepSeek 路径。现有 DeepSeek 底层模块可以留在独立源码边界中等待后续决定，但本构建不提供 DeepSeek UI、网络调用或可由前端调用的 DeepSeek 配置/切换命令。

## 7. 便捷安装与启动

便捷性是发布门槛，不是后续优化项：

- 用户只运行一个 NSIS 安装程序；
- 默认采用当前用户安装，不要求管理员权限；
- 安装完成页提供“立即启动”；
- 自动创建名称明确的桌面和开始菜单快捷方式；
- 首次启动立即自动检测，不要求用户先进入设置；
- 用户从官方 Codex 安装说明返回软件时自动重新检测，同时保留显式“重新检测”；
- ArcGIS Pro 已找到、Add-In 已处理且 ChatGPT 已登录后，后续启动跳过已完成步骤并直接进入对话；
- 主界面始终保留一个“启动 ArcGIS Pro 并连接”主操作；
- 故障后可在原页面重试，不要求重装软件或编辑配置文件。

B1 的唯一额外前置条件是试用者自行安装 Codex CLI。软件必须在首次使用页清楚说明该前置条件，并链接到官方 OpenAI Codex CLI 安装文档：<https://learn.chatgpt.com/docs/codex/cli>。

## 8. 单页首次使用界面

页面按固定顺序显示五个紧凑区块：

1. **Codex CLI**：正在检测、未安装、版本未经验证、不兼容或已就绪；
2. **ArcGIS Pro 3.7**：正在检测、未找到、路径无效或已找到；
3. **ArcGIS Add-In**：需要安装、已打开安装程序、需要重启或已连接；
4. **ChatGPT**：需要登录、登录进行中或已登录；
5. **启动与自检**：启动 ArcGIS Pro、等待 Bridge/MCP 或进入对话。

未安装 Codex 时显示“查看官方安装说明”和“重新检测”。ArcGIS 自动发现失败时允许通过单文件对话框选择 `ArcGISPro.exe`，取消选择不是错误。已完成区块自动折叠，但仍可展开查看状态。

界面不显示模型提供商选择、DeepSeek 字样、API Key 输入或通用设置中心。

## 9. 错误与恢复

错误映射为稳定、可操作类别：

- `codex_not_found`：安装 Codex CLI 后重新检测；
- `codex_version_unverified`：显示版本警告并继续自检；
- `codex_incompatible`：更新或重装 Codex 后重检；
- `chatgpt_auth_required`：启动浏览器登录；
- `arcgis_not_found`：重新检测或手动选择；
- `addin_action_required`：打开官方 Add-In 安装程序；
- `arcgis_restart_required`：关闭并重新打开 ArcGIS Pro；
- `bridge_reconnecting`：显示正在重连并允许显式重试；
- `mcp_unavailable`：显示本地组件未就绪并允许自检。

不得继续使用笼统的“ArcGIS 连接已过期”作为所有问题的提示。原始异常、令牌、认证数据和不必要的绝对用户路径不显示、不持久化、不进入普通诊断。

## 10. 打包与卸载

预览打包脚本必须可重复构建：

1. 运行现有 .NET、Rust 和前端自动化测试；
2. 发布应用私有 MCP Server；
3. 构建 ArcGIS Pro 3.7 Add-In；
4. 验证 Add-In 不携带 Esri ArcGIS 运行时程序集；
5. 暂存固定命名的应用资源，但不暂存 Codex CLI；
6. 构建 Windows x64 NSIS 当前用户安装包；
7. 输出固定命名安装包和小写 SHA-256 校验文件。

构建配置和测试必须断言：不存在 Codex sidecar、Codex 构建参数、DeepSeek UI/API 资源或硬编码的 `C:\Program Files\ArcGIS`、`D:\arcgis_pro`。

Windows 标准卸载移除本软件安装目录、快捷方式、普通配置和应用私有 `CODEX_HOME`。它不得卸载或删除外部 Codex CLI、Codex 桌面应用、ArcGIS Pro、ArcGIS 工程、地图、图层、地理数据库或其他用户数据。Add-In 卸载继续显示 Esri 官方管理指引，不直接递归操作 AddIns 根目录。

## 11. 测试策略

### 11.1 后端测试

- PATH 与用户 npm 目录候选优先级；
- 候选缺失、非文件、超时、异常退出和畸形版本输出；
- 解析为绝对路径后才执行；
- 已验证版本、未验证但兼容版本和不兼容版本；
- App Server 自检超时和子进程回收；
- 私有 `CODEX_HOME` 注入且不修改父进程环境；
- ChatGPT-only 有效提供商固定为 Codex；
- DeepSeek 命令不进入前端可调用命令表；
- 现有 Codex、MCP、ArcGIS 生命周期和 17 工具契约不回归。

### 11.2 前端测试

- 新用户只看到 ChatGPT 路径；
- Codex 缺失时打开官方说明并可重新检测；
- 窗口重新获得焦点时自动重检；
- 版本警告不阻止通过的兼容性自检；
- ArcGIS 自动发现、手动选择、Add-In 重启提示和一键启动；
- 已完成步骤折叠，日常启动直接进入对话；
- 分层状态替代“ArcGIS 连接已过期”；
- 页面不存在 DeepSeek、API Key 或提供商切换控件。

### 11.3 打包与人工烟雾测试

- 安装包只有一个，可从桌面和开始菜单启动；
- 安装完成可立即启动应用；
- 包内不存在 Codex 二进制；
- 没有 Codex 时能完成指引—返回—重检流程；
- 外部 Codex 版本不同但协议兼容时可登录并继续；
- 使用新应用私有 `CODEX_HOME` 完成 ChatGPT 浏览器登录；
- 自动发现或手动选择 ArcGIS Pro 3.7；
- 通过官方方式处理 Add-In 后建立 Bridge/MCP 连接；
- 验证 17 个工具清单，并实测健康检查、读取工程、列出地图/图层；
- 卸载后外部 Codex、ArcGIS Pro、工程和 GIS 数据保持不变。

## 12. 发布产物与完成标准

交付内容：

- `ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe`；
- 对应 SHA-256 文件；
- 简明安装与首次使用手册；
- 第三方许可证和来源说明；
- 自动化验证报告和人工烟雾测试记录。

完成必须同时满足：

1. Task 1–4 已验收能力不回归；
2. 安装、快捷方式、首次启动和日常启动路径达到第 7 节便捷性要求；
3. ChatGPT 浏览器登录、ArcGIS Pro 发现和基础工具链可在真实机器完成；
4. 安装包不携带 Codex CLI，也不出现 DeepSeek 用户入口；
5. 所有自动化测试、生产构建、打包验证和要求的人工烟雾测试通过；
6. 卸载只影响本软件拥有的文件和私有数据。

## 13. 风险与后续边界

外部 Codex 会独立更新，因此版本号本身不能保证协议兼容。版本警告加实际 App Server 自检是 B1 下的主要保护；检测失败时必须阻止进入不可用对话状态，并给出恢复路径。

DeepSeek 后续若恢复，应重新进行独立设计、计划和验收，再添加 API、Key、工具循环和提供商 UI。本修订不得被解释为已实现或承诺 DeepSeek。
