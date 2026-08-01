同意拆成 A、B 两个独立计划，C 暂缓。

A 是基础执行能力，B 建立在 A 之上。两者虽然都接管浏览器，但目标和交互方式完全不同：

| 计划 | 核心目标 | 页面理解方式 | 执行特点 |
|---|---|---|---|
| A：单任务极速执行 | 尽快完成用户明确任务 | 只理解完成任务所需的页面范围 | 少检查、少往返、快速执行 |
| B：探索测试 | 系统发现功能和边界问题 | 先建立页面功能地图 | 先调研、提问、制定计划，再探索 |
| C：TabStride Case | 复用历史任务 | 暂不确定 | 暂缓 |

# A 计划：单任务极速执行

## 目标

> 用户提出明确任务后，TabStride 直接接管当前浏览器，以最少的 AI 思考轮次、CLI 进程和浏览器往返完成任务。

例如：

- 填写并提交表单；
- 创建一条数据；
- 修改一个配置；
- 验证输入框类型和长度限制；
- 执行一组明确的页面操作；
- 检查一个明确结果。

## A1. 建立真正的快速路径（A-1 已完成，2026-07-31）

正常流程应该收敛为：

```text
直接 attach + 初始页面信息
→ 生成执行步骤
→ 一次 Flow 或持久连接执行
→ 最终验证
→ stop
```

不再默认执行：

```text
status
→ doctor
→ browsers
→ session start
→ tab list
→ snapshot
```

这些命令只在失败后用于诊断。

建议让 attach 一次返回：

```text
session_id
browser_id
tab_id
url
title
document_version
initial_snapshot
capabilities
```

可以扩展现有命令：

```bash
tabstride session start \
  --mode attach \
  --tab active \
  --snapshot
```

或者提供更直接的入口：

```bash
tabstride attach --snapshot
```

### A-1 实施结果：取消正常任务的诊断预检

- 正常自动化的第一个服务请求固定为目标 `session start`，不再预先调用
  `status`、`doctor`、`browsers`、`session list` 或 `tab list`；
- CLI 业务命令直接发送目标 RPC，服务未运行时只提示启动 `tabstride serve`，不会自动启动；
- 移除每次 `session start` 重复执行的 Agent Skill 文件系统同步；Skill 只在
  `tabstride serve` 启动后异步同步，不阻塞任务快速路径；
- `session start` 的多浏览器错误直接携带候选实例，Agent 使用错误内候选重试，避免额外
  `browsers` 往返；
- 无浏览器连接时直接提示连接插件；只有无法由结构化错误恢复时才使用 `status`/`doctor`；
- Skill 将诊断命令明确标记为失败恢复工具，并增加契约测试防止后续退化；
- 集成测试验证 attach 正常路径的第一帧就是 `session.start`，不存在健康检查预检。

真实 Chrome 基准回归（每种模式 20 次，排除冷启动样本）通过：

- attach 单任务：总耗时 P50 `128.0 ms`、P95 `225.6 ms`；attach P50 `8.1 ms`；
- isolated 单任务：总耗时 P50 `288.1 ms`、P95 `353.8 ms`；
- 两种模式均低于 A-0 设定的 P95 回归上限。

## A2. 区分确定任务和自适应任务

### 确定任务

页面已知，步骤之间不依赖未知结果：

```text
attach + snapshot
→ 一个 Flow
→ stop
```

例如创建 Todo、填写固定表单、执行回归步骤。

### 自适应任务

下一步依赖页面返回内容：

```text
attach + snapshot
→ 持久客户端
→ 操作 + 页面增量
→ 下一步操作
→ 最终验证
→ stop
```

此时不能把每一步拆成新的 CLI 进程，应复用现有持久客户端。

## A3. 减少 Snapshot 次数

现在 AI 常见的低效模式是：

```text
click
→ snapshot
→ fill
→ snapshot
→ click
→ snapshot
```

建议动作结果可选携带：

```json
{
  "result": {},
  "document_changed": true,
  "document_version": 18,
  "snapshot_delta": {}
}
```

这样 AI 可以通过一次响应同时得到：

- 操作是否成功；
- 页面是否变化；
- 哪些节点新增或删除；
- refs 是否仍然有效；
- 是否真的需要完整 Snapshot。

只有以下情况才重新获取完整 Snapshot：

- 页面导航；
- 当前 Locator 失效；
- 页面发生大量重建；
- AI 无法从增量结果判断下一步；
- 操作失败需要 Evidence。

## A4. 优化 AI 的执行决策

TabStride Skill 应明确规定：

- 不预先运行 `status`；
- 不预先运行 `doctor`；
- 直接 attach，失败后再诊断；
- 两个以上确定动作优先 Flow；
- 不为了观察日志而拆分 Flow；
- 已知 Case 不要求初始 Snapshot；
- 未知页面最多先做一次 Snapshot；
- 使用语义 Locator，不反复查询 DOM；
- 最终验证放入 Flow Assertions；
- 用户取消后立即终止；
- 成功或失败都必须释放 attach。

## A5. 性能指标

除了测量单条命令，还要测量完整任务：

- 从收到用户任务到第一次浏览器操作的时间；
- 整个任务完成时间；
- AI 与 TabStride 的往返次数；
- 启动的 CLI 进程数量；
- Snapshot 获取次数；
- Full AX Tree 获取次数；
- Flow 总耗时；
- Flow 每一步耗时；
- Locator、Actionability、CDP 各阶段耗时；
- 缓存命中率。

一个确定任务的理想调用数量是：

```text
1 次 attach
1 次 flow.run
1 次 stop
```

## A 计划验收用例

建立一组固定基准：

1. TodoMVC 添加并完成三个任务。
2. 创建一条表单数据。
3. 验证输入框必填、类型和长度。
4. 登录态后台修改一个字段。
5. 页面 DOM 重建后继续操作。
6. 操作过程中用户点击取消。
7. Locator 失效后使用增量页面状态恢复。

每次记录总时间、往返次数和 Snapshot 次数。
