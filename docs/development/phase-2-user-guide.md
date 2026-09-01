# Phase 2 开发与使用手册

本文是 ArcGIS Pro 智能助手 `0.1.0` Phase 2 的完整开发/用户手册。Phase 2 已提供受约束的 R0 读取和 R1 临时选择/导航能力；不会保存项目、编辑源数据、执行任意 SQL/脚本或暴露通用操作分派器。实时 ArcGIS Pro 验收仍以[Phase 2 手工冒烟清单](phase-2-smoke.md)为准，自动测试通过不代表清单中的 GUI 项目已经观察通过。

## 活动工作树和解决方案

- 活动工作树：`E:\Ai_Project\codex\MCP-Server-ArcGIS-Pro-AddIn\.worktrees\arcgis-pro-agent-foundation`
- Visual Studio 解决方案：`E:\Ai_Project\codex\MCP-Server-ArcGIS-Pro-AddIn\.worktrees\arcgis-pro-agent-foundation\McpServer.sln`

在 PowerShell 中进入活动工作树后，可运行 `devenv .\McpServer.sln`，或在 Visual Studio 中选择“打开项目或解决方案”并打开上述 `McpServer.sln`。不要从父仓库的其他工作树混合构建或安装输出。

## 前置条件

- Windows 当前用户会话；ArcGIS Pro 3.7，并可解析其 `bin\ArcGIS.Core.dll`。
- .NET 8 与 .NET 10 SDK；Add-In 使用 .NET 10 Windows 目标，MCP/共享库使用 .NET 8。
- 当前 Visual Studio（支持上述 SDK）以及 ArcGIS Pro SDK 随安装提供的 MSBuild targets。
- Node.js/npm、Rust/Cargo，以及可执行的 `codex.cmd`。
- 可用的 ChatGPT 订阅账号。应用只发起官方 ChatGPT 浏览器登录，不接受 API Key。

若 ArcGIS Pro 不在默认位置，后续命令显式传入 `-ArcGISProInstallDir D:\arcgis_pro`，或换成经 `Resolve-ArcGISProInstall.ps1` 验证的实际根目录。

## 受保护的自动验证

先关闭 ArcGIS Pro，确认临时 no-register guard 路径不存在，再在活动工作树根目录运行完整非 GUI 门禁：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

该脚本运行安装器安全测试、缓存可用的 .NET 测试、前端测试/生产构建、Tauri debug no-bundle 构建和 Rust 测试，并核对 17 个 MCP 工具、16 个 Add-In 能力、文档清单、非破坏性元数据和禁止面。它通过不存在的 `ArcGISFolder` 防止 ArcGIS SDK 自动注册，且不会启动 ArcGIS Pro 或执行 GIS 操作。

只做快速源码门禁时可运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -WorkspaceRoot . -SourceAssertionsOnly
```

这不是完整验收的替代品。

## 事务型开发安装

自动验证通过后，开发者可明确选择执行安装：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Install-Dev.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

安装器先验证源码、安装根和 Add-In 根的实体路径不重叠且不经过重解析点/GIS 数据容器；随后在系统临时目录暂存、计算 SHA-256，通过固定事务日志和同目录原子替换提交。它只覆盖清单已经拥有且摘要/长度/文件身份仍匹配的文件，失败时回滚或在下次运行继续安全恢复。不要删除 `.arcgis-pro-agent-install.lock` 来判断或解除锁；真正的排他性来自安装器持有的文件句柄。

默认安装结果：

| 用途 | 默认位置 |
| --- | --- |
| 桌面程序 | `%LOCALAPPDATA%\ArcGISProAgent\dev\0.1.0\desktop\arcgis-pro-agent-desktop.exe` |
| MCP 程序与依赖 | `%LOCALAPPDATA%\ArcGISProAgent\dev\0.1.0\mcp` |
| Add-In 包 | `%USERPROFILE%\Documents\ArcGIS\AddIns\ArcGISPro\{1A0481EA-3F43-4C98-B4B5-A58C727CD115}\ArcGISProAgent.AddIn.esriAddinX` |
| 所有权清单 | `%LOCALAPPDATA%\ArcGISProAgent\dev\install-manifest.json` |
| 事务日志/锁 | `%LOCALAPPDATA%\ArcGISProAgent\dev` 下的安装器固定文件 |

`-AddInRoot` 是精确的包拥有目录，固定包名直接位于其下，不附加 `0.1.0` 子目录。从旧默认目录迁移时，安装器只删除旧严格清单已经拥有且哈希、长度、身份、所有者和版本仍匹配的 Add-In；清理同样只作用于当前清单拥有的包，绝不处理 ArcGIS Pro 的 `AssemblyCache`。这里仅说明可发现的安装拓扑，不宣称 Add-In 已通过后续 GUI 实时加载验收。

不要手工复制部分输出覆盖已安装版本，也不要把 GIS 项目或数据放入安装根。

## 启动顺序与 ChatGPT 登录

1. 启动已安装的 `arcgis-pro-agent-desktop.exe`，等待 Codex 状态就绪。
2. 点击“使用 ChatGPT 账号登录”。只在系统浏览器打开的 `auth.openai.com`、`chatgpt.com` 或 `openai.com` 官方 HTTPS 页面完成订阅登录；不要向应用粘贴 API Key。
3. 启动 ArcGIS Pro 3.7，打开一个空白或可丢弃且无需保存的项目，确认“ArcGIS Pro 智能助手桥接”Add-In 已加载。
4. 回到桌面应用，等待 ArcGIS MCP 状态 ready。右侧上下文每 10 秒在窗口可见、已登录、有活动会话且 MCP ready 时刷新。
5. 先观察项目、活动地图/场景/布局、范围和嵌套图层摘要，再发送自然语言请求。

若 ArcGIS Pro 已先启动，桌面应用会显示断开；启动桌面程序后重新加载 Add-In 或等待重连即可。桌面只通过已认证的 Codex App Server/MCP 路径工作，不直接调用命名管道。

### MCP 启动与窗口兼容契约

- 新建对话后，桌面端会主动调用 `mcpServerStatus/list` 发现 ArcGIS MCP；无需先发送一条模型消息。
- 当前 Codex 使用 `mcpServer/startupStatus/updated`；旧事件名只作为内部兼容别名，不代表存在第二套 MCP 状态。
- 显式 `ARCGIS_AGENT_MCP_COMMAND` 优先；开发安装默认自动使用同版本 `mcp\ArcGISProAgent.Mcp.exe`，无需修改全局 PATH。
- 150% 缩放时右侧上下文自动改为抽屉；输入区应始终可见。

## 连接诊断

- **Codex 未就绪**：运行 `codex.cmd --version`；PowerShell 执行策略拦截 `.ps1` shim 时始终使用 `.cmd`。
- **MCP 未 ready**：检查同一版本的 MCP 是否安装完整，查看非敏感日志；不要公开本机路径或凭据材料。
- **Add-In 断开**：确保桌面程序与 ArcGIS Pro 由同一 Windows 用户、同一权限级别运行，并来自同一 `0.1.0` 安装。
- **`protocol_mismatch`**：重新运行事务安装，使桌面/MCP/Add-In 都使用协议 `1.0`。
- **上下文过期**：连接调用失败会保留最后一次安全摘要并标记过期；上下文或图层调用失败会保留摘要、保留当前连接健康并标记上下文 stale。隐藏窗口、切换账号/会话或 MCP readiness 变化会拒绝迟到响应。
- **没有项目/活动地图**：这是当前空上下文，不是读取失败；无项目时不会调用项目工具，布局活动时不会调用图层列表。

## 17 个公开工具

R0 是只读操作；R1 只改变当前选择或视图等临时交互状态，不保存项目或编辑源数据。Phase 2 没有 R2/R3 工具。

<!-- phase-2-tools:start -->
| 风险 | 工具 | 用途 | 自然语言示例 |
| --- | --- | --- | --- |
| R0 | `arcgis_connection_status` | 读取桌面、MCP、Add-In/ArcGIS Pro 连接健康 | “检查 ArcGIS Pro 是否已连接。” |
| R0 | `arcgis_capabilities` | 列出当前精确能力与风险元数据 | “列出当前可用的 ArcGIS 工具。” |
| R0 | `arcgis_describe_context` | 读取项目、项目项、活动视图和范围摘要 | “描述当前项目和活动视图。” |
| R0 | `arcgis_list_layers` | 读取活动地图的扁平嵌套图层树 | “列出所有嵌套图层，并保留层级。” |
| R0 | `arcgis_describe_layer` | 按稳定图层 URI 读取图层/数据源摘要 | “描述 URI 为 layer://… 的图层。” |
| R0 | `arcgis_list_fields` | 按稳定图层 URI 读取字段结构 | “列出道路图层字段及类型。” |
| R0 | `arcgis_count_features` | 用可选类型化谓词计数 | “统计 STATUS 等于 Open 的要素数。” |
| R0 | `arcgis_query_features` | 返回指定字段的有界分页记录 | “从道路图层查询 NAME 和 CLASS，最多 20 条。” |
| R0 | `arcgis_query_spatial` | 以源图层、显式范围或当前视图作有界空间查询 | “查询当前视图内与地块相交的道路，最多 20 条。” |
| R0 | `arcgis_get_selection` | 读取全部或指定图层的选择计数和有界 OID 样本 | “汇总当前选择集，不要改变选择。” |
| R1 | `arcgis_select_by_attribute` | 以类型化谓词 Replace/Add/Remove/Toggle 选择 | “用 Replace 选择 STATUS 等于 Open 的道路。” |
| R1 | `arcgis_select_by_location` | 以有界空间源 Replace/Add/Remove/Toggle 选择 | “把当前视图内相交道路 Add 到现有选择。” |
| R1 | `arcgis_clear_selection` | 清除指定图层或活动地图选择 | “清除活动地图的全部选择。” |
| R1 | `arcgis_activate_view` | 按稳定项目项 URI 激活已有地图/场景/布局 | “激活 item://… 对应的已有布局。” |
| R1 | `arcgis_zoom_to_layer` | 缩放到图层或仅选中要素 | “缩放到道路图层中已选中的要素。” |
| R1 | `arcgis_zoom_to_extent` | 缩放/平移到有限且规范化的范围 | “缩放到 xmin…ymax…、WKID 4326 的范围。” |
| R1 | `arcgis_flash_features` | 临时闪烁有界 OID 集合 | “在道路图层闪烁这些已确认的 OID 一秒。” |
<!-- phase-2-tools:end -->

工具卡只显示中文标签、工具名、R0/R1/unknown、规范化结果、时长、允许的聚合计数/布尔摘要和公开错误码。它不显示原始参数、记录、路径、连接信息或任意错误消息；内存最多保留最近 100 张卡片。

## 稳定 URI 工作流

不要依据显示名称猜测目标。先调用 `arcgis_describe_context` 获得地图/场景/布局项目项 URI，再用 `arcgis_list_layers` 获得图层 URI 和 parent URI。把返回的稳定 URI 原样交给后续 describe、field、query、selection、activate、zoom 或 flash 工具；项目切换或图层不存在后重新枚举。名称和 long name 只用于人类确认，不是身份键。

## 谓词、空间源与结果边界

- 属性谓词只接受字段名、受支持的比较运算和一个 JSON 标量；`IsNull`/`IsNotNull` 不带值。运算为 `Equal`、`NotEqual`、`GreaterThan`、`GreaterThanOrEqual`、`LessThan`、`LessThanOrEqual`、`StartsWith`、`Contains`、`IsNull`、`IsNotNull`，并会按实际字段类型关闭式验证。不能提交 SQL、where clause、表达式或脚本。
- 空间关系只接受 `Intersects`、`Within`、`Contains`、`Touches`、`Crosses`、`Overlaps`。空间源必须三选一：`Layer` 只带源图层 URI，`Extent` 只带有限且 `xmin < xmax`、`ymin < ymax` 的范围，`CurrentView` 不带 URI/范围。未知空间参考会失败；源图层参与合并的要素最多 1,000 个。
- 选择组合模式仅为 `Replace`、`Add`、`Remove`、`Toggle`。空匹配时 Replace 清空目标图层，其他模式保持不变；不会自动重试非幂等选择/闪烁调用。
- 查询 `limit` 为 1–100（默认 20），`offset` 不得为负；请求字符串最多 2,000 字符，flash OID 最多 100 个且必须为正数、唯一，闪烁时长最多 10 秒。
- 单次公开查询记录预算为 900 KiB，Bridge 响应帧最多 1 MiB；不支持原生 offset 的数据源最多扫描 10,000 行后关闭式失败。上下文项目项最多 100、活动地图图层最多 200；桌面轮询每次 MCP 调用最多等待 5 秒。

## 当前限制

- 没有新增/编辑/删除要素、保存/另存项目、创建地图/布局、地理处理、分析、符号化、导出或数据源修复。
- 没有任意 SQL、WKT/CIM、脚本、Shell、命令执行、通用操作 ID 或未知动态工具。
- R1 为临时交互且不保存；本阶段不提供审批 UI。任何 MCP elicitation 仍关闭式拒绝，直到 Phase 3 实现明确审批。
- 右侧上下文仅保留经过字段白名单和界限检查的摘要，不获取字段 schema 或要素值。实时 ArcGIS 行为尚需按 smoke 清单逐项观察。

## 安全停止与恢复

正常停止顺序为先关闭桌面应用，再关闭 ArcGIS Pro。异常时只结束本应用启动的桌面、Codex App Server 和 MCP 进程，不要结束其他 Codex/ArcGIS 会话。重新启动桌面应用并重新加载 Add-In 后再观察连接恢复；不要手工编造运行时文件。

当前没有卸载命令。若必须清理开发安装，先检查所有权清单顶层和文件条目 owner，再只处理清单列出的已拥有应用/Add-In 文件。不要递归删除 `%LOCALAPPDATA%\ArcGISProAgent`、ArcGIS 项目、`.gdb`、`.sde`、shapefile、栅格或其他源数据。

## 后续功能如何增加或移除

功能变更必须作为完整垂直切片完成：先更新受版本控制的 Contracts DTO/校验和 `CapabilityCatalog` 风险元数据，再添加/移除 Add-In operation 与 `RuntimeOperationIds`/dispatcher 精确分支，随后更新显式 MCP tool 注册、桌面本地风险/中文标签白名单、单元/集成安全测试、本文工具表和 `Test-Foundation.ps1` 的精确集合。新增 R2/R3 必须在后续阶段同时实现预览、明确审批、备份/恢复和失败语义，不能把它伪装为 R0/R1。

移除功能时反向删除所有注册点、能力、dispatcher 分支、文档和测试，并让精确集合门禁先红后绿；禁止保留隐藏别名、兼容 dispatcher 或旧 `pro.*` 路径。
