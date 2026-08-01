可以把 Rust 的 `crate` 理解成“一个可独立编译的软件包”。TabStride 通过 Cargo 把多个 crate 编译、链接成一个名为 `tabstride` 的可执行文件。

```text
Rust 源码
├── tabstride-protocol    协议依赖库
└── tabstride             CLI、daemon、入口程序
          ↓ Cargo 编译与链接
target/release/tabstride
          ↓ 压缩
tabstride-v0.1.7-平台.tar.gz
          ↓ install.sh 安装到 PATH
~/.local/bin/tabstride
```

## 1. 哪个 crate 会变成命令行工具？

关键配置在 [crates/tabstride-cli/Cargo.toml](/Users/channelwill/cwpro/github/TabStride/crates/tabstride-cli/Cargo.toml)：

```toml
[package]
name = "tabstride"

[[bin]]
name = "tabstride"
path = "src/main.rs"
```

它告诉 Cargo：

- 这个 Rust 包叫 `tabstride`
- 要生成一个名为 `tabstride` 的可执行文件
- 程序入口是 `src/main.rs`

因此，真正对应命令行工具入口的是：

[crates/tabstride-cli/src/main.rs](/Users/channelwill/cwpro/github/TabStride/crates/tabstride-cli/src/main.rs)

其中的：

```rust
fn main() -> ExitCode {
    // 解析命令并执行
}
```

相当于 Java 的 `public static void main`，或者 Node.js CLI 的入口文件。

## 2. `tabstride-protocol` 为什么没有单独安装？

`tabstride-cli` 声明了这个依赖：

```toml
[dependencies]
tabstride-protocol = { workspace = true }
```

对应：

[crates/tabstride-protocol](/Users/channelwill/cwpro/github/TabStride/crates/tabstride-protocol)

编译时，Cargo 会：

1. 编译 `tabstride-protocol`
2. 编译 CLI 和 daemon 代码
3. 把它们链接到同一个 `tabstride` 可执行文件中

所以用户不需要单独安装 `tabstride-protocol`。

该 crate 还定义了一个开发辅助程序 `dump-schema`，用于生成 JSON Schema，但它不会被放进最终发布压缩包。

## 3. CLI 和 daemon 是两个程序吗？

不是。它们在同一个二进制文件里。

例如：

```bash
tabstride status
tabstride session start
tabstride click @e1 --session abcd
tabstride serve
```

这些命令执行的都是同一个 `tabstride` 文件，只是传入了不同子命令。

大致调用关系是：

```text
src/main.rs
  ↓ 解析参数
Cli / Command
  ├── status
  ├── session
  ├── click
  └── serve
        └── 启动显性的前台常驻服务
```

命令行参数解析使用的是 Rust 的 `clap` 库。

## 4. 本地如何编译？

在项目根目录执行：

```bash
cargo build -p tabstride
```

会生成开发版：

```text
target/debug/tabstride
```

可以直接运行：

```bash
./target/debug/tabstride --version
./target/debug/tabstride --help
```

构建正式发布版：

```bash
cargo build --release --locked -p tabstride
```

会生成：

```text
target/release/tabstride
```

这里：

- `--release`：启用正式版本优化
- `--locked`：严格使用 `Cargo.lock` 中锁定的依赖版本
- `-p tabstride`：只构建名为 `tabstride` 的 package 及其依赖

根目录还配置了正式构建优化：

```toml
[profile.release]
strip = "symbols"
lto = "thin"
codegen-units = 1
```

主要作用是减少文件大小并改善性能。

## 5. Skill 如何进入可执行文件？

构建前，[build.rs](/Users/channelwill/cwpro/github/TabStride/crates/tabstride-cli/build.rs) 会把根目录的：

```text
skill/SKILL.md
```

同步到 CLI crate 内部。

随后这段代码：

```rust
pub const DEFAULT_SKILL_MD: &str =
    include_str!("../../skill/SKILL.md");
```

会在编译期间把 `SKILL.md` 内容嵌入可执行文件。

所以用户执行：

```bash
tabstride install-skill
```

不需要旁边额外放一份 `SKILL.md`，二进制自身已经包含它。

## 6. GitHub 如何生成不同系统的安装包？

发布流程位于：

[.github/workflows/release-cli.yml](/Users/channelwill/cwpro/github/TabStride/.github/workflows/release-cli.yml)

推送类似标签：

```text
cli-v0.1.7
```

GitHub Actions 会分别构建：

- macOS Apple Silicon
- macOS Intel
- Linux x64
- Linux ARM64
- Windows x64

核心构建命令是：

```bash
cargo build --release --locked \
  -p tabstride \
  --target aarch64-apple-darwin
```

不同 `--target` 会产生对应平台的二进制。然后打包成：

```text
tabstride-v0.1.7-aarch64-apple-darwin.tar.gz
tabstride-v0.1.7-x86_64-apple-darwin.tar.gz
tabstride-v0.1.7-x86_64-unknown-linux-musl.tar.gz
tabstride-v0.1.7-x86_64-pc-windows-msvc.zip
```

macOS/Linux 包中只有一个主要文件：

```text
tabstride
```

Windows 包中是：

```text
tabstride.exe
```

## 7. `install.sh` 做了什么？

[install.sh](/Users/channelwill/cwpro/github/TabStride/install.sh) 本身不编译 Rust。它只负责：

1. 判断操作系统和 CPU 架构
2. 查询最新版本
3. 下载匹配的发布压缩包
4. 解压 `tabstride`
5. 安装到：

```text
~/.local/bin/tabstride
```

6. 确保 `~/.local/bin` 位于 `PATH`

安装完成后，在任意目录输入：

```bash
tabstride --version
```

Shell 会通过 `PATH` 找到这个文件并运行。

需要特别区分：这里打包的只是 CLI/daemon；Chrome 扩展是另一个 TypeScript/WXT 构建产物，需要单独打包和安装。
