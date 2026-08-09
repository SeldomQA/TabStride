
## 1. 浏览器插件包是否通用？

**通用，不区分操作系统/架构。** 扩展是纯 Web 技术（HTML/JS/CSS），在任何平台的 Chrome 上运行结果完全相同。

唯一要区分的是**浏览器**而非操作系统：`pnpm zip` 默认打的是 Chrome MV3 包（`tabstride-0.2.0-chrome.zip`），适用于 Chrome/Edge 等 Chromium 内核浏览器。Firefox 需要 `wxt zip -f firefox`（当前项目未配置 Firefox 支持，manifest 里用了 `debugger` CDP 权限，Firefox 兼容性需要另行验证）。

## 2. CLI 如何打不同系统的包？

CLI 是 Rust 原生二进制，**必须区分平台**。仓库的 [release-cli.yml](file://d:/github/TabStride/.github/workflows/release-cli.yml) 已配置 5 个目标矩阵：

| 平台 | target | 打包格式 |
|---|---|---|
| macOS Intel | `x86_64-apple-darwin` | tar.gz |
| macOS Apple Silicon | `aarch64-apple-darwin` | tar.gz |
| Linux x64 | `x86_64-unknown-linux-musl` | tar.gz |
| Linux ARM64 | `aarch64-unknown-linux-musl` | tar.gz |
| Windows x64 | `x86_64-pc-windows-msvc` | zip |

**不要尝试本地交叉编译**（在 Windows 上打 macOS 包几乎不可行），直接用 GitHub Actions 在对应平台的 runner 上原生构建。本地只能可靠打出当前平台：

```powershell
cargo build --release -p tabstride   # 只产出当前平台的 target/release/tabstride.exe
```

## 3. 如何发布到 GitHub Releases？

**完全自动化，只需打 tag 推送**。当前版本两边都是 `0.2.0`，且还没有任何 tag，流程如下：

```powershell
cd d:\github\TabStride

# 发布 CLI（触发 Release CLI workflow，自动构建 5 个平台并发布）
git tag cli-v0.2.0
git push origin cli-v0.2.0

# 发布扩展（触发 Release Extension workflow）
git tag ext-v0.2.0
git push origin ext-v0.2.0
```

要点：

- **版本守卫**：tag 版本号必须与 `Cargo.toml`（CLI）/ `apps/extension/package.json`（扩展）中的 `version` 一致，否则 workflow 会直接失败。下次发版需先改代码里的版本号再打对应 tag（如 `cli-v0.3.0`）
- **CLI Release 产物**：5 个平台压缩包 + `version.json`（供 `tabstride update` 自更新和 `install.sh`/`install.ps1` 安装脚本使用），并标记为 latest
- **扩展 Release 产物**：`tabstride-extension-v0.2.0-chrome.zip`（用户下载解压后按开发者模式加载）
- 也可以不推 tag，在 GitHub 仓库 **Actions** 页面手动 Run workflow（会用代码里当前的版本号）
- 两个 workflow 都支持 `workflow_dispatch`，发错了可以删 tag 重发

**发布前建议**：确保工作区改动（F-2 修复尚未提交）先 commit + push，否则 tag 指向的 commit 不包含最新代码。需要我现在帮你提交 F-2 并打 tag 发布吗？