# TabStride 开发计划

> 目标：把 TabStride 建设成一个能够快速接管真实浏览器、具备 Playwright 级执行可靠性，并能把 Agent 操作沉淀为可复用用例的开源测试运行时。

本文是 TabStride 下一阶段的主开发计划。

- `develop-plan.md` 保留为早期产品路线参考；
- `develop-plan2.md` 保留为 Playwright 能力研究参考；
- 本文统一性能、可靠执行和用例工程化三条产品主线，并给出实施顺序和验收标准。

## 一、产品定位

TabStride 不以替代 Playwright Test 为目标，也不做通用 browser-use 推理框架。

TabStride 重点解决下面的问题：

1. 用户已经在真实 Chrome 中打开页面，Agent 可以随时原地接管；
2. Agent 发出操作后，TabStride 能尽快执行，不把时间浪费在重复启动进程、连接和固定等待上；
3. 浏览器操作具有稳定的定位、等待、断言和证据能力；
4. 一次有价值的探索任务可以沉淀为结构化用例，后续无需再次逐步描述；
5. 用例可以继续在 TabStride 中快速执行，也可以导出为 Playwright Test；
6. 核心协议保持 Agent 无关，不绑定 Codex、MCP 或其他单一平台。

产品定位可以概括为：

> TabStride = 真实浏览器接管体验 + Playwright 级执行语义 + Agent 用例资产化。

## 二、三条产品主线

### 2.1 极速浏览器执行

核心目标：排除 Agent 思考和网站响应时间后，TabStride 自身不成为明显瓶颈。

重点能力：

- `tabstride serve` 常驻服务；
- Agent 与 Serve 长连接；
- Extension 与 Serve 长连接；
- 原地 Attach 当前标签页；
- 批量 Flow；
- 增量 Snapshot；
- Locator、Document 和 Overlay 节点缓存；
- 每层性能埋点；
- 操作完成后返回必要的页面变化，减少额外 Snapshot。

### 2.2 Playwright 级可靠执行

核心目标：学习 Playwright 成熟的测试行为，但不把 `playwright-core` 嵌入扩展。

重点能力：

- Locator；
- Strict Matching；
- Actionability；
- Auto Wait；
- Web-first Assertions；
- Network/Console Evidence；
- Screenshot、Snapshot 和操作日志；
- Playwright Test 导出。

底层继续使用 TabStride 自己的 Chrome Extension、CDP 和协议实现。

### 2.3 用例工程化

核心目标：第一次由 Agent 探索，第二次开始优先复用结构化用例。

重点能力：

- `tabstride init` 项目脚手架；
- Case Schema；
- Session Action Trace；
- 从 Session 生成 Case Draft；
- 参数化和数据驱动；
- Case 校验、执行和升级；
- Evidence Bundle；
- Playwright Test 导出；
- SKILL、Case、Data 和 Evidence 分层管理。

## 三、总体架构

### 3.1 目标架构

```text
Agent / IDE / CI
├── TabStride Native Client
├── tabstride CLI
├── MCP Adapter
├── TypeScript SDK
└── Python SDK
          │
          │ JSON-RPC / WebSocket / Local IPC
          ▼
      tabstride serve
      ├── Agent Connection Registry
      ├── Browser Registry
      ├── Session / Tab Lease
      ├── Command Queue
      ├── Flow Runtime
      ├── Snapshot / Locator Cache
      ├── Action Trace
      ├── Evidence Collector
      └── Metrics / Diagnostics
          │
          │ Persistent WebSocket
          ▼
TabStride Chrome Extension
      ├── Attach / Tab Control
      ├── Locator Resolver
      ├── Actionability Engine
      ├── Auto Wait
      ├── Assertion Engine
      ├── Network / Console Observer
      └── CDP Driver
          │
          ▼
       Current Chrome
```

### 3.2 `tabstride serve` 与现有 daemon 的关系

不要维护两套重复服务。

现有 daemon 内核应逐步升级为统一的 Serve 内核：

```text
tabstride serve
    唯一正式入口；前台运行，日志可见，适合开发和 Agent 长连接

tabstride daemon start
    已废弃并从帮助隐藏，仅暂时保留兼容
```

二者必须：

- 使用同一套核心实现；
- 共享协议和配置；
- 使用相同的浏览器连接方式；
- 通过进程锁防止重复实例；
- 能被 `tabstride status` 和 `tabstride doctor` 统一诊断。

仅增加 `tabstride serve` 命令但仍让 Agent 每次启动一次 CLI，不视为完成性能改造。

### 3.3 Agent 无关协议

TabStride Core Protocol 不绑定 MCP。

推荐：

- 本地核心协议：JSON-RPC 2.0；
- Agent 长连接：WebSocket；
- CLI 快速访问：Unix Domain Socket / Windows Named Pipe；
- MCP、Codex Plugin 和其他 Agent 接入作为 Adapter；
- 协议版本独立于 CLI、Extension 和 Adapter 版本。

## 四、性能设计

### 4.1 性能边界

每次任务必须区分三类时间：

```text
总耗时
├── Agent 思考与编排时间
├── 网站网络与业务响应时间
└── TabStride Runtime 时间
```

TabStride 只对 Runtime 时间设硬性 SLO，但报告中必须同时展示三类时间，避免互相掩盖。

### 4.2 分层性能埋点

每条命令至少记录：

```text
agent_received_at
serve_queue_entered_at
serve_queue_started_at
extension_sent_at
extension_received_at
cdp_started_at
cdp_finished_at
extension_replied_at
serve_replied_at
```

派生指标：

- Agent → Serve 调度；
- 队列等待；
- Serve → Extension WebSocket；
- Extension dispatch；
- CDP 执行；
- Locator 解析；
- Actionability 等待；
- Snapshot 构造；
- 响应序列化；
- 整体 Runtime。

CLI 增加：

```bash
tabstride click ... --timing
tabstride flow run ... --timing
tabstride metrics summary
tabstride metrics export --out metrics.json
```

### 4.3 性能目标

排除 Agent 思考和网站响应时间后：

| 操作 | 目标 |
|---|---:|
| Agent → Serve 调度 | P95 ≤ 20ms |
| Attach 当前标签 | P95 ≤ 500ms |
| Click / Fill / Press | P95 ≤ 300ms |
| 300 节点完整 Snapshot | P95 ≤ 800ms |
| 增量 Snapshot | P95 ≤ 300ms |
| Session Stop | P95 ≤ 500ms |
| 本地 5 步 Flow | P95 ≤ 2s |
| 已验证 Case 启动 | P95 ≤ 500ms |

所有指标必须同时记录 P50、P95、P99 和样本数。

### 4.4 性能基准场景

固定维护以下 Benchmark：

1. 空白静态页 Click；
2. 100、300、1000 AX 节点 Snapshot；
3. 输入框 Fill + Press；
4. DOM 每 100ms 更新时的 Actionability；
5. 五步 TodoMVC Flow；
6. TrackingMore Add Package Flow；
7. 单条 CLI 与长连接 Client 对比；
8. 冷启动、热启动和 Extension 重连。

每次性能相关 PR 必须提供前后对比，不能只报告“感觉更快”。

### 4.5 优先消除的固定成本

- Session Start 不再同步扫描和更新所有 Agent Skill；
- 已有 Serve 时，CLI 不重复执行完整 daemon 验证；
- 没有 Serve 时，CLI 直接失败并提示 `tabstride serve`，不自动启动后台进程；
- Agent Client 保持连接，不为每步创建新进程；
- Snapshot 的 Overlay backend node IDs 按 Document 缓存；
- 同一 Locator 在 Document 未变化时允许复用解析缓存；
- 操作响应携带轻量页面变化，减少无意义 Snapshot；
- 禁止在 Runtime 内默认使用固定 `sleep`；
- 自动等待由事件、MutationObserver、Network 和短间隔检查共同驱动。

## 五、Attach 与权限模型

### 5.1 Session Mode

保留三种模式：

```text
isolated   独立 Agent Window
attach     原地控制用户明确授权的现有标签
workspace  在当前窗口的 Agent Tab Group 中工作
```

主要命令：

```bash
tabstride session start --mode attach --tab active
tabstride session start --mode attach --tab-id 123
tabstride session start --mode isolated
tabstride session start --mode workspace --current-window
```

### 5.2 Tab Lease

所有读写权限统一由 Tab Lease 控制：

```ts
interface TabLease {
  tabId: number;
  sessionId: string;
  mode: "isolated" | "attached" | "borrowed" | "created";
  canRead: boolean;
  canWrite: boolean;
  canEvaluate: boolean;
  canNavigate: boolean;
  canObserveNetwork: boolean;
}
```

验收要求：

- Attach 不创建窗口和空白页；
- Attach 不移动标签；
- Session Stop 不关闭用户标签；
- 未授权标签不可写；
- 同一标签不能被两个 Session 同时写入；
- 用户可以随时 Stop；
- 标签关闭、浏览器断线和 Serve 重启后 Lease 自动清理；
- 权限确认失败时 fail-closed。

## 六、可靠执行内核

### 6.1 执行流水线

所有交互统一经过：

```text
Target Input
→ Locator Resolve
→ Strict Match
→ Actionability Wait
→ Action Dispatch
→ Post-action Observation
→ Trace / Evidence
```

CLI、Flow、Case 和 SDK 不得各自实现一套不同的点击或等待逻辑。

### 6.2 Locator Schema

第一阶段支持：

```yaml
target:
  by: role
  role: button
  name: Add Package
  exact: true
```

Locator 类型：

- `ref`；
- `role + accessible name`；
- `label`；
- `placeholder`；
- `text`；
- `testId`；
- `css`。

后续再考虑：

- parent/child 组合；
- `nth`，默认不推荐；
- Frame Locator；
- Shadow DOM 组合定位；
- 自定义 Test ID attribute。

严格匹配规则：

```text
0 个匹配：not_found
1 个匹配：继续
多个匹配：ambiguous_target
```

探索阶段允许使用 Snapshot Ref；保存为 Case 时应尽量转换为稳定语义 Locator。

### 6.3 Actionability

Click 自动检查：

- attached；
- visible；
- stable；
- enabled；
- receives events；
- 未被页面元素遮挡；
- 点击点位于可交互区域。

Fill 自动检查：

- attached；
- visible；
- enabled；
- editable；
- 输入类型允许目标值。

Check/Select/Press 应分别定义自己的 Actionability 条件。

当页面变化导致节点失效时，应在超时范围内通过原 Locator 重新解析，而不是继续使用陈旧 backendNodeId。

### 6.4 Auto Wait

自动等待必须内置在 Action 和 Assertion 中。

支持状态：

- attached / detached；
- visible / hidden；
- enabled / disabled；
- editable；
- stable；
- value；
- checked；
- URL；
- text；
- response；
- DOM idle；
- network idle，谨慎使用。

默认原则：

- 等待具体业务条件，避免固定毫秒数；
- 超时返回最后一次实际状态；
- 返回等待阶段和耗时；
- 用户 Stop 能立即取消等待；
- Flow 总超时和单步超时分别控制。

### 6.5 Assertions

第一阶段实现：

```text
visible / hidden
text equals / contains / matches
value equals
enabled / disabled
checked / unchecked
count
url equals / matches
response status
```

Assertion Schema 示例：

```yaml
assert:
  target:
    by: label
    label: Carriers
  property: value
  equals: dhl-germany
  timeout: 5s
```

失败结果：

```json
{
  "passed": false,
  "assertion": "value equals",
  "expected": "dhl-germany",
  "actual": "",
  "timeout_ms": 5000,
  "last_locator_result": "matched",
  "evidence_id": "ev_01"
}
```

Agent 不得通过自动修改 expected 让失败变绿。

## 七、Flow Runtime

### 7.1 目标

Flow 用于一次提交多个确定步骤，减少 Agent、CLI 和 Serve 的往返。

Flow 不是任意 JavaScript 执行器。第一阶段只接受受控动作和断言。

### 7.2 Flow Schema

```yaml
name: add-package
timeout: 30s

steps:
  - click:
      target:
        by: role
        role: button
        name: Add Package

  - fill:
      target:
        by: label
        label: Tracking number
      value: RT397955885DE

  - wait_for:
      target:
        by: label
        label: Carriers
      state: populated

  - click:
      target:
        by: role
        role: button
        name: Add
        exact: true

  - assert:
      target:
        by: text
        text: RT397955885DE
      property: visible
```

### 7.3 Flow 行为

- 默认按顺序执行；
- 失败立即停止；
- 每步支持独立超时；
- 支持变量；
- 支持有限条件分支，第一版可暂缓；
- 支持人工介入步骤；
- 高风险操作可要求执行前确认；
- 返回成功步骤、失败步骤、耗时和 Evidence；
- 用户 Stop 立即取消整个 Flow；
- 不允许某一步失败后由 Agent静默改写断言并继续。

### 7.4 Flow API

```bash
tabstride flow validate add-package.yaml
tabstride flow run add-package.yaml --session abcd
tabstride flow run add-package.yaml --var tracking_number=RT397955885DE
```

长连接协议：

```json
{
  "jsonrpc": "2.0",
  "id": "flow-01",
  "method": "flow.run",
  "params": {
    "session_id": "abcd",
    "flow": {},
    "variables": {}
  }
}
```

## 八、Evidence 与可诊断性

### 8.1 Evidence 内容

第一阶段记录：

- 每一步开始、结束和耗时；
- 输入的 Locator；
- Locator 匹配数量和最终目标；
- Actionability 各项检查；
- 操作前后 URL；
- 失败时 AX Snapshot；
- 失败截图；
- Console error；
- Network request/response 摘要；
- 人工介入与用户结果；
- TabStride、Extension、Chrome 和协议版本。

### 8.2 Evidence Bundle

```text
evidence/<run-id>/
├── report.json
├── report.html
├── actions.jsonl
├── assertions.jsonl
├── snapshots/
├── screenshots/
├── network.jsonl
├── console.jsonl
├── timings.jsonl
└── metadata.json
```

默认规则：

- 成功步骤只保留轻量记录；
- 失败步骤自动保留完整现场；
- Response Body 有大小限制；
- Cookie、Authorization、密码字段默认脱敏；
- Evidence 默认本地保存；
- 支持过期清理；
- Evidence 不等于 Case，不应混入用例目录版本控制。

### 8.3 Network 与 Console

第一阶段命令：

```bash
tabstride network list
tabstride network wait --url "**/tracking/create"
tabstride network get <request-id>
tabstride console list --level error
```

暂不在第一阶段实现 Network Mock，避免扩大写入面和复杂度。

## 九、用例工程化

### 9.1 项目脚手架

新增：

```bash
tabstride init trackingmore-tests
```

生成目录：

```text
trackingmore-tests/
├── tabstride.config.yaml
├── SKILL.md
├── cases/
│   └── example.case.yaml
├── data/
│   └── example.data.yaml
├── fixtures/
├── evidence/
├── generated/
│   └── playwright/
├── .gitignore
└── README.md
```

初始化参数：

```bash
tabstride init trackingmore-tests --template web-testing
tabstride init trackingmore-tests --template browser-task
tabstride init . --force-empty
```

`--force-empty` 只允许在没有冲突文件的目录使用，不覆盖用户内容。

### 9.2 资产职责

| 资产 | 职责 |
|---|---|
| `SKILL.md` | 业务知识、约束、登录、数据准备和人工协作说明 |
| `cases/` | 可执行步骤、变量和断言 |
| `data/` | 测试数据、边界值和矩阵 |
| `fixtures/` | 可选的前置与清理流程 |
| `evidence/` | 每次执行结果，默认忽略版本控制 |
| `generated/` | 导出的 Playwright Test 等派生产物 |

SKILL 不承担确定性执行步骤；Case 不重复书写长篇业务知识。

### 9.3 Case Schema

```yaml
schema: tabstride.case/v1
name: add-package
description: Add one tracking number
status: draft
tags: [package, smoke]

session:
  mode: attach

preconditions:
  - User is logged in
  - Packages page is open

variables:
  tracking_number:
    type: string
    required: true
    sensitive: false

steps:
  - click:
      target:
        by: role
        role: button
        name: Add Package

  - fill:
      target:
        by: label
        label: Tracking number
      value: "{{tracking_number}}"

  - wait_for:
      target:
        by: label
        label: Carriers
      state: populated

  - click:
      target:
        by: role
        role: button
        name: Add
        exact: true

assertions:
  - visible:
      target:
        by: text
        text: "{{tracking_number}}"

evidence:
  network: errors
  console: errors
  screenshots: on-failure
```

### 9.4 Case 生命周期

```text
draft
→ validated
→ stable
→ deprecated
```

- `draft`：从探索过程生成，尚未确认；
- `validated`：Schema 正确且至少成功执行一次；
- `stable`：由用户确认具有长期复用价值；
- `deprecated`：页面或业务已经废弃。

状态转换必须显式发生，不能因为一次运行成功自动变成 stable。

### 9.5 Case CLI

```bash
tabstride case list
tabstride case show add-package
tabstride case validate add-package
tabstride case run add-package --var tracking_number=RT397955885DE
tabstride case run add-package --data data/tracking-numbers.yaml
tabstride case capture --from-session abcd --name add-package
tabstride case promote add-package --status stable
tabstride case export add-package --format playwright
```

### 9.6 从 Session 沉淀 Case

完整流程：

```text
用户描述任务
→ Agent 首次探索
→ Serve 记录结构化 Action Trace
→ 用户确认任务有复用价值
→ case capture 生成 Draft
→ Agent 将输入参数化
→ Agent 补充明确断言
→ Case Validate
→ 使用另一组数据重放
→ 用户确认后提升为 Stable
```

生成 Case 时必须：

- 优先使用语义 Locator；
- 去掉纯观察、诊断和重复 Snapshot；
- 识别可参数化输入；
- 保留人工介入点；
- 不自动推断不明确的业务期望；
- 不保存密码、Token 和验证码；
- 对高风险提交步骤保留确认策略。

### 9.7 已有 Case 的执行策略

已有 Case 默认由 Runtime 确定性执行，不再让 Agent 逐步决定每个动作。

只有以下情况返回 Agent 或用户：

- Locator 找不到或匹配多个；
- Assertion 失败；
- 页面结构明显改变；
- 数据准备失败；
- 需要验证码、登录或人工确认；
- 发生业务上不可逆的高风险操作；
- Case 明确配置了 Agent 决策点。

## 十、Playwright Test 导出

### 10.1 定位

Playwright 导出是资产迁移能力，不是 TabStride Runtime 的底层依赖。

```text
TabStride Case
├── TabStride：真实浏览器、人工协作、快速复用
└── Playwright Test：独立浏览器、CI、跨浏览器稳定回归
```

### 10.2 导出规则

导出内容包括：

- 语义 Locator；
- 输入变量和测试数据；
- 明确 Assertions；
- 必要等待；
- Network Response 断言；
- 人工步骤 TODO 注释；
- 前置条件注释；
- TabStride 不支持直接转换部分的说明。

禁止默认导出：

- 临时 Snapshot Ref；
- backendNodeId；
- TabStride Overlay 节点；
- 无解释的固定等待；
- 密码、Token、Cookie 和验证码；
- 从未经过用户确认的业务断言。

### 10.3 命令

```bash
tabstride case export add-package \
  --format playwright \
  --out generated/playwright/add-package.spec.ts
```

导出后执行格式化和静态校验，但不默认安装 Playwright 或下载浏览器。

## 十一、阶段开发计划

以下按 2 名核心工程师估算，共约 16～20 周。阶段可以局部重叠，但依赖关系不能颠倒。

### Phase 0：基线与计划收敛，1 周

交付：

- 确认本文为主路线；
- 建立 Timing Schema；
- 建立 TodoMVC 和 TrackingMore Benchmark；
- 记录当前 CLI、daemon、Extension 各层耗时；
- 给 Snapshot、Click、Fill、Session Start 增加分段埋点；
- 保存首份性能基线报告。

验收：

- 能解释一次命令时间花在哪一层；
- Benchmark 可重复运行；
- 性能 PR 有统一对比方法。

### Phase 1：Fast Runtime，3～4 周

交付：

- `tabstride serve`；
- Serve 与 daemon 共用内核；
- Agent WebSocket API；
- 持久 Native Client；
- Attach 当前标签；
- Tab Lease；
- Session Start Skill Sync 异步化或缓存；
- Overlay/Snapshot 节点缓存；
- Flow Runtime v1；
- Stop 和取消贯穿长连接与 Flow。

验收：

- 用户手动启动 `tabstride serve` 后 Extension 保持在线；
- Agent 建立一次连接后连续执行至少 100 条命令；
- 中间不重复启动 CLI；
- Attach 不创建和移动标签；
- 五步 TodoMVC Flow 单次提交完成；
- 达到本计划的基础性能 SLO；
- Serve 或 Extension 重连后状态可诊断、无幽灵 Session。

### Phase 2：Reliable Execution，4～5 周

交付：

- Locator Schema v1；
- role/name/label/placeholder/text/testId/css/ref；
- Strict Matching；
- Click/Fill/Press/Select Actionability；
- Locator 重解析；
- Auto Wait；
- Assertions v1；
- Flow 中使用统一执行内核；
- 失败 Evidence。

验收：

- 延迟出现的元素无需 Agent 重试；
- 被遮挡、禁用和不稳定元素给出正确诊断；
- DOM 重建后 Locator 能在超时内重新找到目标；
- 多匹配不会静默点击第一个；
- Assertion 自动重试并返回实际值；
- CLI、Flow 和 SDK 的行为一致。

### Phase 3：Case Engineering，4～5 周

交付：

- `tabstride init`；
- `tabstride.config.yaml`；
- Case Schema v1；
- Case Validate/Run/List/Show；
- 变量和数据驱动；
- Action Trace；
- `tabstride case capture`；
- Case 生命周期；
- Evidence Bundle；
- SKILL 模板。

验收：

- 新项目可在 10 秒内生成；
- 首次探索 Session 可生成 Draft Case；
- Draft 不包含诊断噪声和敏感数据；
- 更换参数后无需 Agent 逐步思考即可重放；
- Case 失败可定位到具体步骤并提供证据；
- Schema 支持向后兼容和迁移提示。

### Phase 4：Evidence 与 Playwright Export，2～3 周

交付：

- Console Collector；
- Network Collector；
- Evidence Report；
- Playwright Exporter v1；
- 导出格式化和静态校验；
- TrackingMore Case 导出示例。

验收：

- 失败步骤自动关联 Console/Network/Screenshot；
- Case 可导出可读的 Playwright Test；
- 导出使用语义 Locator；
- 不泄露敏感数据；
- 不支持转换的步骤有明确 TODO，而不是静默丢失。

### Phase 5：Ecosystem，2 周起

交付：

- TypeScript SDK；
- Python SDK；
- MCP Adapter；
- Codex Plugin；
- 多 Agent SKILL 生成；
- HTML Report Viewer。

核心协议稳定前不要同时开发过多 Adapter。

## 十二、首个完整垂直切片

以 TrackingMore Add Package 作为三条主线的联合验收场景。

### 12.1 首次探索

```text
1. 用户手动打开 TrackingMore Packages
2. 手动启动 tabstride serve
3. Agent Attach 当前标签
4. 使用语义 Locator 点击 Add Package
5. 输入 tracking number
6. 自动等待 Carrier
7. 点击 Add
8. 捕获创建请求和 Console Error
9. 断言列表出现 tracking number
10. Session Stop，标签保持原位
```

### 12.2 沉淀用例

```text
1. 从 Session Action Trace 生成 add-package Draft
2. 将 tracking number 参数化
3. 删除探索期多余 Snapshot
4. 保留 Carrier 和列表断言
5. 使用另一单号 Validate
6. 用户确认后提升为 Stable
```

### 12.3 快速复用

```bash
tabstride case run add-package \
  --var tracking_number=CB774959417DE
```

执行过程不再逐步询问 Agent。仅在失败、验证码或页面变化时返回。

### 12.4 Playwright 导出

```bash
tabstride case export add-package \
  --format playwright \
  --out generated/playwright/add-package.spec.ts
```

### 12.5 对比指标

验收报告同时记录：

1. Agent 首次探索总耗时；
2. TabStride Runtime 耗时；
3. Stable Case 重放耗时；
4. Playwright Test 执行耗时；
5. 三种方式的成功率和失败诊断质量。

核心成功标准：

> 第一次由 Agent 帮助用户探索；第二次开始，无需重复描述任务，也无需 Agent 逐步思考。

## 十三、测试策略

### 13.1 单元测试

- Locator 解析和严格匹配；
- Actionability 状态机；
- Auto Wait 超时和取消；
- Assertion 比较器；
- Flow Schema 和执行器；
- Case Schema、变量和敏感字段；
- Playwright 导出转换；
- Timing 聚合。

### 13.2 协议测试

- CLI / Native Client / Adapter 使用同一协议契约；
- 版本兼容；
- 未知方法和字段；
- 断线重连；
- 重复请求 ID；
- Flow 取消；
- Session 所有权；
- 多 Agent 并发。

### 13.3 真实 Chrome E2E

- Serve 启动与 Extension 连接；
- Attach 当前标签；
- Isolated Session；
- Locator；
- Actionability；
- Auto Wait；
- Flow；
- Case Run；
- 用户 Stop；
- Extension Service Worker 重启；
- Chrome 断线；
- Serve 重启；
- 标签关闭；
- 未授权标签拒绝；
- TabStride Overlay DOM 隔离；
- Snapshot 排除 TabStride 内部节点。

Actionability 第一批真实 Chrome 回归由
`pnpm e2e:chrome` 执行。测试使用一次性 Chrome Profile 加载生产扩展构建，
走完整的 CLI → Serve/IPC → WebSocket → Extension → CDP 链路；专用页面位于
`tests/e2e/actionability/`，覆盖延迟出现、enabled 变化、持续移动、遮挡恢复、
DOM 重建、多匹配、超时 Evidence、用户 Stop、Attach 标签隔离和 CLI/Flow 一致性。

### 13.4 性能回归测试

- 每日运行固定 Benchmark；
- P95 退化超过 15% 触发告警；
- Snapshot 大页面单独监控；
- 冷启动和热路径分开；
- 不用一次偶然快结果替代统计数据。

Actionability 热路径由 `pnpm perf:chrome` 固定采集 20 个有效样本（另有预热），
分别记录单条 CLI click 和 Flow click+assert 的 P50/P95/P99。结果与
`tests/e2e/actionability/performance-baseline.json` 比较，P95 超过基线 15%
即失败，并将完整 JSON 报告写入 `artifacts/`。

## 十四、安全与隐私

- Serve 默认只监听 localhost；
- Agent API 和 Extension 使用不同身份；
- 首次配对生成 Token；
- Token 可撤销和轮换；
- 每个 Session 声明能力；
- Attach 必须获得目标 Tab Lease；
- 密码、Cookie、Authorization 和验证码默认不进入 Trace；
- Case Capture 自动过滤敏感输入；
- `evaluate` 保持高风险能力，不进入默认 Case；
- Network Body 有大小、类型和脱敏限制；
- 高风险业务提交支持用户确认；
- 审计日志默认保存在本地。

## 十五、兼容与迁移

### 15.1 现有 CLI

已有命令保持可用：

```text
tabstride session
tabstride snapshot
tabstride click
tabstride fill
tabstride press
...
```

它们逐步改为统一调用 Serve Core，不要求用户立即切换 SDK 或 Flow。

### 15.2 协议版本

- 新字段优先可选；
- Flow 和 Case Schema 使用独立版本；
- 破坏性变更提供迁移器；
- CLI、Serve 和 Extension 版本不一致时给出明确诊断；
- 不允许静默忽略不支持的 Assertion 或 Step。

### 15.3 配置迁移

`tabstride init` 生成的配置必须记录 Schema Version：

```yaml
schema: tabstride.project/v1
```

提供：

```bash
tabstride project doctor
tabstride project migrate
```

## 十六、主要风险

### 16.1 过早做太多 Adapter

风险：Core Protocol 尚未稳定就同时维护 MCP、Codex、Python 和 TypeScript。

措施：先完成 Native Client 和 CLI，稳定后再扩展 Adapter。

### 16.2 复制 Playwright 复杂度

风险：Actionability 和 Locator 逐渐变成重写整个 Playwright。

措施：只实现 TabStride 使用频率最高的行为，保持明确兼容边界，不承诺 Playwright API 完全兼容。

### 16.3 Case 生成不可靠

风险：Agent 从一次偶然成功操作生成脆弱 Case。

措施：Draft 状态、语义 Locator、第二组数据验证和用户确认缺一不可。

### 16.4 Evidence 过重

风险：每一步截图和保存 Response Body 反而拖慢执行。

措施：成功轻量、失败完整；Evidence 等级可配置；所有采集开销进入 Timing。

### 16.5 长连接状态复杂

风险：Serve、Extension 和 Agent 重连后出现幽灵 Session、重复命令或错误 Lease。

措施：请求幂等 ID、Session Owner、心跳、租约过期、重连状态机和真实 Chrome 故障测试。

### 16.6 为速度牺牲安全

风险：为了减少确认步骤而扩大默认权限。

措施：性能优化不能绕过 Tab Lease、用户 Stop、敏感数据过滤和高风险确认。

## 十七、优先级 Backlog

### P0

- 性能分层埋点；
- `tabstride serve` Core；
- Agent 长连接；
- Attach 当前标签；
- Tab Lease；
- Flow Runtime v1；
- Locator Schema v1；
- Actionability 基础；
- Stop/Cancel 全链路；
- 真实 Chrome E2E。

### P1

- Auto Wait；
- Web-first Assertions；
- 增量 Snapshot；
- `tabstride init`；
- Case Schema；
- Case Run；
- Action Trace；
- Case Capture；
- Evidence Bundle；
- Network/Console Error。

### P2

- 数据驱动；
- Playwright Exporter；
- HTML Report；
- TypeScript SDK；
- Python SDK；
- MCP Adapter；
- Codex Plugin。

### P3

- Network Mock；
- 视频录制；
- 高级 Trace Viewer；
- 复杂条件 Flow；
- 分布式浏览器；
- 远程执行；
- 自动失败最小化。

## 十八、Definition of Done

一个功能只有同时满足以下条件才算完成：

- Core、CLI/Client 和 Extension 契约明确；
- 有单元测试；
- 有协议测试；
- 涉及浏览器行为时有真实 Chrome E2E；
- 有取消和超时行为；
- 有结构化错误；
- 有性能数据；
- 不泄露敏感信息；
- CLI Help 和文档已更新；
- 新能力能被 Flow 或 Case 复用，而不是只存在于单条命令中。

## 十九、最终成功标准

TabStride 下一阶段是否成功，用以下问题判断：

1. 能否在不创建、不移动标签的情况下原地接管当前 Chrome 页面？
2. 除网站和 Agent 时间外，TabStride 是否达到可量化的低延迟目标？
3. Locator、等待、操作和断言是否足够稳定，而不是依靠 Agent 不断重试？
4. 用户是否可以把一次成功任务沉淀为 Case？
5. 第二次运行相同任务时，是否不再需要逐步描述和 Agent 逐步思考？
6. 失败时是否能明确区分产品缺陷、环境问题、数据问题和人工阻塞？
7. 有长期回归价值的 Case 是否可以导出为可读的 Playwright Test？
8. TabStride 是否仍然保持开源、Agent 无关和用户可随时中止？

最终形成的资产链路是：

```text
快速接管
→ 可靠执行
→ 形成用例
→ 快速复用
→ 必要时导出 Playwright
```

TabStride 不应止步于“更强的浏览器操作 CLI”，而应成为真实浏览器探索、Agent 协作和自动化测试资产之间的连接层。
