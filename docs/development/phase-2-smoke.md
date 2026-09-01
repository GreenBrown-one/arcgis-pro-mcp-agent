PENDING

## 2026-08-01 基础现场验收：通过

- 使用真实仓库重新构建并事务安装 `0.1.0`；安装清单包含 51 个所有者为 `ArcGISProAgent` 的文件。
- ArcGIS Pro 3.7 正常响应，Add-In、Bridge、Contracts 三个已加载 DLL 均与最新 Release 构建哈希一致。
- 桌面端会在 `thread/start` 前自动创建私有工作区；当前 Codex 状态为“就绪”，ArcGIS 状态为“已连接”，未显示等待、异常或对话创建错误。
- 已安装 MCP 的只读 R0 探针列出 17 个工具和 16 项 Add-In 能力；连接协议为 `1.0`，Add-In 为 `0.1.0.0`，ArcGIS Pro 为 `3.7.0.1901`。
- 当前未打开 ArcGIS 项目，因此 `arcgis_describe_context` 返回安全的不可用状态；未暴露项目路径或数据源路径。
- DPI 144（150%）下窗口为 1942×1256，`ArcGIS 指令` 输入框与发送按钮可见、启用且未超出窗口。
- 本轮没有发送模型 turn，没有创建或保存 ArcGIS 项目，没有调用 R1/R2/R3，也没有修改 GIS 数据。
- 抽屉交互和需要测试项目的 R0/R1 项目仍保持 pending，待后续功能稳定后再执行。

# Phase 2 ArcGIS Pro 3.7 手工冒烟清单

日期：2026-07-29

本清单只能在空白或可丢弃、无需保存的测试项目中由人工观察。当前所有项目均保持 pending；本次编码任务没有安装、启动 ArcGIS Pro、创建项目、调用实时 GIS 工具、改变选择/视图或执行清理。

## 2026-07-30 实机兼容性修复验收

以下项目只能在本次实机验收中直接观察后勾选；源码测试或历史观察不能替代：

- [ ] 新建对话后未发送首个模型 turn，桌面端即主动发现 ArcGIS MCP。
- [x] 未设置 `ARCGIS_AGENT_MCP_COMMAND` 时，已安装桌面端自动使用同版本相邻 `mcp\ArcGISProAgent.Mcp.exe`，无需修改全局 PATH。
- [ ] ArcGIS Pro 3.7 加载 Add-In 后，桌面端显示 ArcGIS ready/已连接状态。
- [x] `arcgis_connection_status` 返回无敏感信息的 R0 健康摘要。
- [ ] 现有上下文刷新返回无项目路径、数据源路径或原始错误的 R0 上下文摘要。
- [ ] DPI 144 / 150% 缩放下输入 composer 完整可见。
- [ ] DPI 144 / 150% 缩放下上下文按钮可打开抽屉，关闭按钮和遮罩均可关闭抽屉。
- [x] 全程沿用 ChatGPT 订阅登录，没有请求或输入 API Key。
- [x] 全程没有保存 ArcGIS 项目。
- [x] 全程没有编辑、创建、删除或覆盖 GIS 数据，也没有调用 R1/R2/R3。

本次直接观察：安装 manifest 为 `0.1.0`、51 项且无缺失；桌面 Codex 命令使用同版本 MCP 绝对路径且没有 API-key 形态；独立短生命周期 R0 探针返回连接成功、协议 `1.0`、Add-In `0.1.0.0`、ArcGIS Pro `3.7.0.1901`。未勾选项仍未通过：桌面没有进入 ready，R0 context 返回安全的不可用状态，DPI 144 窗口恢复后物理尺寸为 1295×837，但视觉证据与抽屉开闭因 GUI 审批额度耗尽而未执行。

另发现安装拓扑缺陷：ArcGIS Pro 实际加载的是默认扫描根中 2026-07-19 的 GUID 缓存版本，三个已加载 DLL 与 2026-07-30 标准安装包全部哈希不同；新包被安装到并行的 `ArcGISProAgent` 根而非 ArcGIS Pro 默认扫描根。未删除、注销或手工修改包/缓存，须先修复安装器目标并事务重装。

## 安装、加载与连接

- [ ] 在关闭 ArcGIS Pro 后执行受保护的非 GUI 门禁，再事务安装同一 `0.1.0` 构建。
- [ ] 桌面程序启动并完成 ChatGPT 订阅登录；没有 API Key 输入。
- [ ] ArcGIS Pro 3.7 加载 Add-In，MCP 变为 ready，连接状态显示协议/版本且无敏感信息。
- [ ] 在空白/可丢弃项目中，右栏显示项目、未保存标记、活动地图/场景/布局、有限范围和 live 标记。
- [ ] 主动断开/关闭 ArcGIS Pro 后上下文标为 stale 且保留最后安全摘要；重新打开/加载后自动恢复 live。

## R0 只读工具

- [ ] `arcgis_connection_status` 返回当前连接健康。
- [ ] `arcgis_capabilities` 精确列出 17 个 Phase 2 工具且只有 R0/R1。
- [ ] `arcgis_describe_context` 返回项目、项目项、活动视图和范围摘要，不暴露项目路径。
- [ ] `arcgis_list_layers(includeNested=true)` 返回父子图层，右栏折叠树按深度显示嵌套关系、可见性和类型。
- [ ] `arcgis_describe_layer` 仅用稳定 layer URI 描述测试图层。
- [ ] `arcgis_list_fields` 返回测试图层字段类型摘要。
- [ ] `arcgis_count_features` 使用与字段类型兼容的 typed predicate，并拒绝原始 SQL。
- [ ] `arcgis_query_features` 只返回请求字段、稳定 OID、`limit <= 100` 的有界页面与 hasMore。
- [ ] `arcgis_query_spatial` 以显式有限 extent 为 source 完成有界查询。
- [ ] `arcgis_query_spatial` 以 current view 为 source 完成有界查询。
- [ ] `arcgis_query_spatial` 以 source layer URI 为 source 完成有界查询。
- [ ] `arcgis_get_selection` 返回每个图层计数和有界 OID 样本，且不改变选择。

## R1 临时选择

- [ ] `arcgis_select_by_attribute` 的 Replace、Add、Remove、Toggle 各观察一次实际最终计数。
- [ ] `arcgis_select_by_location` 的 Replace、Add、Remove、Toggle 各观察一次实际最终计数。
- [ ] 空匹配时属性/位置 Replace 清空目标图层，Add/Remove/Toggle 不改变现有选择。
- [ ] `arcgis_clear_selection` 分别清除指定图层和活动地图选择。

## R1 临时视图与闪烁

- [ ] `arcgis_activate_view` 按稳定项目项 URI 激活一个已存在地图。
- [ ] `arcgis_activate_view` 按稳定项目项 URI 激活一个已存在场景。
- [ ] `arcgis_activate_view` 按稳定项目项 URI 激活一个已存在布局。
- [ ] `arcgis_zoom_to_layer(selectedOnly=false)` 缩放至图层。
- [ ] `arcgis_zoom_to_layer(selectedOnly=true)` 缩放至已选要素。
- [ ] `arcgis_zoom_to_extent` 缩放/平移至有限且规范化的测试范围。
- [ ] `arcgis_flash_features` 对最多 100 个已验证 OID 临时闪烁，并返回实际 flashedCount。

## 不变量与收尾

- [ ] 全程没有出现 R2/R3、审批 UI、任意 SQL/脚本/命令/通用 dispatcher。
- [ ] 工具卡只显示风险、结果、时长、聚合摘要和公开错误码；没有原始记录、路径、连接信息或任意错误正文。
- [ ] 没有保存测试项目，没有创建项目副本，没有持久化选择或视图状态。
- [ ] 没有编辑、创建、删除或覆盖任何源数据；没有手工删除 GIS 数据。
- [ ] 先安全停止桌面应用，再关闭 ArcGIS Pro；记录非敏感观察结果后才可逐项勾选。
