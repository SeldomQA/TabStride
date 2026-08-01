
---

# C 计划：暂缓

TabStride Case 暂时只保留方向，不进入开发。

当前不确定的问题包括：

- 用户是否愿意维护 YAML；
- 成功轨迹如何转换成稳定 Locator；
- 哪些数据应成为变量；
- 页面变化后应该严格失败还是允许 AI 修复；
- Case 与 Flow 是否需要合并；
- Case 更偏个人任务复用还是团队回归测试。

这些问题可以在 A、B 大量真实使用后，通过实际执行历史再决定。

## 推荐实施顺序

1. 完成 A 计划的快速 attach 和零预检路径。
2. 增加动作响应中的 Document Version 和 Snapshot Delta。
3. 增加 Flow Step Timing 和任务级性能基准。
4. 优化 Skill，让 AI 默认走最快执行路径。
5. 在 A 稳定后开始 B 的页面侦察和功能地图。
6. 实现探索问题、探索计划和 Coverage Ledger。
7. 积累实际任务后再重新评估 C 计划。