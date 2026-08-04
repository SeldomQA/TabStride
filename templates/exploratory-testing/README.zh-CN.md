# TabStride 探索性测试模板

一份方法论模板，教会 AI Agent 如何使用 [TabStride](https://github.com/SeldomQA/TabStride)
进行系统化的探索性测试。

## 这是什么？

这**不是**代码库或 TabStride 插件。它是一份结构化的工作流指南（SKILL.md），
AI Agent 加载后即获得探索性测试的专业方法论。它定义了：

- 如何在不修改数据的前提下安全观察 Web 应用
- 测试前应向用户提出哪些问题
- 如何规划和排列测试场景的优先级
- 如何跟踪覆盖率和记录发现
- 如何输出最终的结构化报告

## 快速开始

1. 安装 [TabStride](../../README.zh-CN.md) 并确保浏览器扩展已连接。
2. 将 `SKILL.md` 复制或引用到 Agent 的 skill 目录中。
3. 在目标网页上对 Agent 说："探索并测试这个页面"。

Agent 将遵循 SKILL.md 中定义的五阶段生命周期：

```
侦察 → 提问 → 计划 → 执行 → 报告
```

## 文件结构

```
templates/exploratory-testing/
├── SKILL.md                    ← Agent 加载此文件（方法论）
├── examples/
│   ├── feature-map.md          ← 阶段 1 输出示例
│   ├── exploration-plan.md     ← 阶段 3 输出示例
│   ├── coverage-ledger.md      ← 阶段 4 跟踪示例
│   └── report.md               ← 阶段 5 输出示例
└── README.md                   ← 给人看的说明
```

## 何时使用本模板 vs. 确定性执行

| 场景 | 使用方式 |
|------|----------|
| "填写这个表单并提交" | 确定性执行（Flow） |
| "执行这 10 个步骤并断言" | 确定性执行（Flow） |
| "测试这个页面，找出 bug" | **探索性测试（本模板）** |
| "探索管理后台模块" | **探索性测试（本模板）** |
| "对新版本做冒烟测试" | **探索性测试（本模板）** |

## 前置要求

- TabStride CLI 已安装且 daemon 运行中
- 浏览器扩展已连接
- Chrome 中已打开待测试的网页

## 自定义

SKILL.md 设计为可按需调整：

- 针对大型应用调整范围限制（最大页面数、导航深度）
- 为每次探索任务调整时间盒和场景数量预算
- 添加领域特定的提问类别（如合规性、无障碍）
- 扩展发现结构，增加团队需要的字段（如 Jira ID）
- 修改报告格式以匹配组织的模板

## 与 TabStride 核心的关系

本模板使用 TabStride 作为基础设施，但不修改它。
它所依赖的全部能力（snapshot、持久客户端、断言、Evidence、截图）
均为 TabStride 核心版本的一部分。
