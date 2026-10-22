# Dioxus 离线练习

Rust 2024 / Dioxus 0.7.10 桌面应用。默认黑暗主题、固定范围模式、空输入。主页三个面板互斥展开，保留原有约 280ms 开合动画与输入校验；切换主题、进入练习和返回主页均保留表单内容。重启应用恢复默认值。

## 题库与练习

- 内嵌 300 道完整单选题，每条 JSON **仅有** `question`、`options`（A～D 顺序的四个字符串）、`answer`（单字母 A～D）。显示题号由数组位置加一派生。
- 第 275、283 题题干相同但选项不同，按原始顺序保留两条，不去重。原始答案原样保留；结构验证不代表答案事实正确。
- JSON 和 CSS 通过 `include_str!` 编译进程序，运行不读取 Excel 或外部 JSON。更新题库必须重新构建。**内嵌不是加密**，有能力的使用者仍可从程序中提取题库。
- 启动解析采用严格 schema，拒绝未知/缺失字段、非四选项、空题库、空题干/选项和非法答案。失败时界面显示错误，不 panic，也不静默回退空库。
- 固定范围使用 1 基编号、包含两端，按原题顺序练习；起止均须在 1～实际题库总数之间，起始不大于结束。随机数量也须在此范围内，不重复抽题，不改动选项顺序。
- 点击「开始」执行当前模式校验；错误留在主页并显示提示。第一击立即锁定答案，只计分一次。答对显示约 400ms；答错显示正确字母与完整选项约 2 秒。
- 反馈后当前题向左离开约 300ms，下一题从右进入约 300ms，进入结束后才可作答。最后一题反馈和离开后显示结果。
- 结果包含本轮总题数、答对数量、正确率（答对/总数 × 100%，**一位小数**）。「再练一次」重置计分并使用同一配置，随机模式重新抽样；「返回主页」保留输入和主题。
- 定时任务属于练习组件作用域；退出和重开显式取消任务，状态推进额外检查会话代号、题序与阶段。异步等待不持有 signal guard。减少动态效果时停用位移动画，但保留反馈阅读时间；推进不依赖 animationend。
- 单人练习状态只在内存中，与联网房间独立。

## 局域网房间（当前实现与边界）

- 创建需要名字（去首尾空白后 1～9 字符）、房间名称（2～6 字符）和四位 ASCII 数字代码。加入先搜索，列表按真实房间 ID 区分；同号房间由用户显式选择。可手动填写房主界面显示的 `IP:端口`。
- 等待室由房主选择固定范围/随机数量，以及「同题同步」或「各自进度」；在线成员准备后开始。题序只生成一次并复制，A～D 不重排。开始后拒绝新的成员；现有内存会话成员可断线自动重连。
- 同步模式等所有在线成员回答再揭晓约两秒后推进；失联约 1.8 秒后不再阻塞。独立模式按本人进度，正确约400ms、错误约2秒，再经过平移动画。成绩表实时列出全部成员，完成与在线独立，已完成离开保留，未完成离开明确标记。
- 不需要中心服务器。UDP IPv4 广播及组播 `239.255.42.39:42739` 发现，四秒过期；TCP 监听系统分配端口，成员间维持连接、约300ms交换副本并检测失联。发现失败不影响手动 IP。防火墙需要允许该 UDP 端口及程序的 TCP 入站；本项目不自动修改防火墙。多网卡可能选错地址，可使用正确网卡 IP 手动连接；校园网客户端隔离可能禁止通信。
- 长度前缀 JSON 每帧最多64KiB；逻辑消息分块、总量最多4MiB，收全才应用。队列有界，最多32成员/32入站连接，握手/空闲读取1.5秒、TCP连接100ms、交换读写200ms超时。相同题库 SHA-256 和协议版本校验。仅用于可信局域网：目前没有 TLS、身份认证、恢复令牌或恶意同网段攻击防护，四位代码不是密码。
- 每个成员创建服务时已有独立监听端口。房主失联后，互通成员按 ID 确定性排序选接替者，增加代次；仅剩一人也继续。正常离开尽力发送副本及接替选择；窗口释放根服务也走尽力离开。网络分区优先可用，不是多数共识。重新连通时按代次及固定优先级统一协调者，合并不可变答案账本；冲突保留本地首次记录并提示，进度分歧取较大题序并提示。不能承诺分区时全局同题或零数据丢失。
- 答案以成员/题序幂等去重，各端根据题库重新计算成绩，不接受对方提供分数。本人待重传提交只在看到其他设备实际副本含该提交后移除；单人时不伪称远端备份。房间及待提交原子写入本地：Windows 为 `%LOCALAPPDATA%\dioxus-app\`（缺失时尝试 `%USERPROFILE%\AppData\Local\dioxus-app\`），其他平台为 `$XDG_DATA_HOME/dioxus-app/`，未设置时为 `~/.local/share/dioxus-app/`；上述环境变量均不可用时回退临时目录。文件按本次服务身份命名。保存先同步文件再替换，Unix 另同步目录，Windows 不提供相同的目录同步保证。保存只保证本机实际收到的信息。
- **尚未完成的计划项**：重启后的稳定身份与存档恢复入口（当前重启是新身份，不能恢复原成员）；显式交接成功确认；安全恢复令牌；分区重连冲突的交互式解决；同步模式联网卡片已有换题入场动画，但没有旧题离场完整时序；准备是本轮单向确认，不能取消，配置修改不重置准备。没有历史管理页面。恢复读取 API 已有及测试，但不代表自动恢复可用。
- 测试使用真实套接字、无 GUI：发现/连接释放、两模式、三人成绩复制/完成房主正常离开后其他人继续完成、两人房主异常停止后唯一幸存者继续、三人连续丢失房主后唯一幸存者完成。异常停止是进程内后台服务/socket 中断，不是多进程 SIGKILL。未验证跨机器 Wi-Fi、GUI 人工流程、网络分区注入、丢确认重试的传输级故障注入、32人满载。当前逐个轮询远端，慢/失联成员多时可使循环超过失联阈值，引发不必要接替，32是输入硬上限而非已验证容量；推荐少量互通成员。同步揭晓截止使用系统时钟，各机器时钟偏差会影响阅读间隔（接替后尤其如此），不承诺精确跨机计时。不可将环回测试表述成已完成这些验收。

## 可重复提取

`scripts/extract_questions.py` 仅使用 Python 标准库 `zipfile` 和 XML 解析，无需安装表格库。读取工作簿第一个工作表，通过表头定位列，处理稀疏单元格、共享字符串和内联字符串，保留中文和字符串中的换行；跳过全空行，拒绝不完整题目或非法答案。不会修改源 Excel，也禁止输出路径等于源路径。输出必须是 `.json`；覆盖已有输出须显式使用 `--force`。

```bash
cd /home/hook/桌面/dioxus-app
uv run python scripts/extract_questions.py "/path/to/人工智能四级（题库）.xlsx" assets/questions.json --force
```

本次完整复现（审计 Markdown 是已转换的只读参考数据，不是运行依赖）：

```bash
uv run python scripts/extract_questions.py \
  /home/hook/.config/CherryStudio/Data/Files/01a1645c-e6de-76cb-b5cc-cd7574e00079.xlsx \
  assets/questions.json --force \
  --audit-markdown /home/hook/.config/CherryStudio/Data/Agents/3cda9f5a-cf7b-46bc-a104-0e70532d9325/tmp/to-markdown/cea11961-3913-4484-8987-2cdf23a23c56.md
```

审计逐条比较全部 300 条题干、四个选项、答案和顺序，全部一致。源文件 SHA256：
`7f5a01fad4b43eef7ecba0368ebad726fa480cad6454d2f01c3e8149e3c2ed29`。

## 启动和验证

```bash
cd /home/hook/桌面/dioxus-app
cargo run --offline --locked

cargo fmt --check
cargo test --offline --locked
cargo build --offline --locked
cargo clippy --offline --locked --all-targets --all-features -- -D warnings
```

首次环境准备需 Rust 工具链及依赖缓存；无需 Dioxus CLI。依赖变更后使用 `cargo generate-lockfile --offline` 更新锁文件，再执行上述 locked 命令。缓存不完整的环境需要先允许 Cargo 下载依赖。

单测覆盖原有 8 项主页输入/面板验证，以及严格题库解析、300 条题库和重复题保留、固定范围首尾/单题/非法范围、随机数量和无重复、重复点击只计分一次、旧会话/旧题序/错误阶段拒绝推进、最后一题结果、全对/全错与 1/3 = 33.3%。构建与单测不等于 GUI 端到端验证；本次未进行交互式目视验证，真实窗口下的计时、动画、焦点、窄窗口、长内容、主题切换与系统减少动态效果仍需人工验收。

## Windows / Linux x64 发行下载

从 [GitHub Releases](https://github.com/QT-hook/300-Questions/releases) 选择已发布版本，下载对应平台压缩包和同名 `.sha256`。只有双平台构建、打包、校验和上传全部成功，Release 才从草稿转为公开；工作流配置本身不代表已有公开产物。

- Windows：`300-Questions-windows-x64.zip` 和 `.zip.sha256`。先用 PowerShell `Get-FileHash .\300-Questions-windows-x64.zip -Algorithm SHA256` 核对，再完整解压 ZIP，运行 `dioxus-app.exe`。需要 [Microsoft Edge WebView2 Evergreen Runtime x64](https://developer.microsoft.com/microsoft-edge/webview2/)；缺少 VCRUNTIME/MSVCP 时安装 [Visual C++ Redistributable x64](https://aka.ms/vs/17/release/vc_redist.x64.exe)。发行版不弹出控制台。
- Linux：`300-Questions-linux-x64.tar.gz` 和 `.tar.gz.sha256`。在下载目录执行 `sha256sum -c 300-Questions-linux-x64.tar.gz.sha256`，再 `tar -xzf 300-Questions-linux-x64.tar.gz`，在解压目录运行 `./dioxus-app`。构建基线是 **Ubuntu 24.04 x86_64 / glibc 2.39**，不是自包含包、AppImage 或全发行版通用程序；旧 glibc 不保证兼容，需要有效的 X11/Wayland 图形桌面和兼容原生运行库。Ubuntu 24.04 可安装：

  ```bash
  sudo apt update
  sudo apt install libgtk-3-0t64 libwebkit2gtk-4.1-0 libjavascriptcoregtk-4.1-0 libxdo3 libssl3t64 libayatana-appindicator3-1 librsvg2-2 xdg-utils fonts-noto-cjk
  ```

  `xdg-utils` 用于系统浏览器打开 GitHub 页脚；中文显示方框时安装 `fonts-noto-cjk` 等 CJK 字体。其他发行版自行确认库名和 ABI 兼容性。
- 两种包均只含程序、中文 `运行说明.txt` 与二进制 `SHA256SUMS.txt`，不含源码、Excel、缓存或外置题库/样式。Linux 解压后可用 `sha256sum -c SHA256SUMS.txt` 校验，Windows 用 `Get-FileHash` 对照。校验和不是数字签名；程序未签名，SmartScreen 等可能提示未知发布者，仅在确认来源后决定运行，不要关闭安全保护。
- 单人练习无需联网。局域网只用于可信网络，无 TLS 或身份认证；需允许程序 TCP 入站和 UDP 42739，不自动修改防火墙。CI 不代表 GUI、动画、WebView2、系统浏览器链接或跨设备局域网已验收，仍须实机验证。
- 所有界面底部共用 GitHub 链接，自然布局推底不覆盖内容；保留主题、表单和动画，链接通过系统浏览器打开。

### 发布流程

`.github/workflows/release.yml` 仅由 `v*` 标签 push 或 `workflow_dispatch` 触发。手动选择分支仅构建、不发布；只有 `refs/tags/v*` 才运行发布 job。发布前由维护者检查版本与标签不存在，再将新标签推送到已确认的提交（例如 `v0.1.0`），不要 force 或复用标签。

两个 build job 仅有 `contents: read`：Windows 稳定 Rust / MSVC x64 复用 `scripts/package-windows.ps1`；Linux 固定 `ubuntu-24.04`、安装原生开发库，构建 `x86_64-unknown-linux-gnu`，调用 `scripts/package-linux.sh`。均先 fmt 和 locked 可移植测试，再 locked release 构建，各上传精确命名的 artifact。

发布 job 等两平台成功才执行，独有 `contents: write`，使用内置 `secrets.GITHUB_TOKEN`，无需另设 PAT。只下载本次运行的两个指定 artifact，严格检查两个包及两个校验文件（总计四个非空文件），验证 SHA-256 后创建 **draft**，全部上传成功才公开。按标签串行，不覆盖已存在的 Release（包括 draft），不用 `--clobber`；上传失败保留草稿供维护者检查，不自动删除或重建已有发行版。

CI 明确跳过真实套接字/发现/时序相关 `room::network_tests`，可移植测试失败会使流程失败。适合的本地网络上执行 `cargo test --locked room::network_tests -- --test-threads=1`。`target/` 和 `dist/` 保持忽略。

### Windows 单平台开发构建（保留原流程）

保留 `.github/workflows/windows-build.yml` 作为分支构建入口；它上传 Actions artifact，不负责公共 Release。公共双平台发行请使用上面的 release workflow。

- `.github/workflows/windows-build.yml` 在任意分支 push 时触发，也支持 Actions 页面 `Windows x64 build` → `Run workflow`（手动入口是否可见取决于默认分支上是否已有此 workflow）。使用 `windows-latest`、稳定 Rust 和 `x86_64-pc-windows-msvc`，先检查格式和可移植测试，再执行 locked release 构建。
- 成功后在该次运行的 **Artifacts** 下载 `300-Questions-windows-x64`（GitHub 可能要求登录）。解压 GitHub 的 artifact 外层包，得到 `300-Questions-windows-x64.zip` 与 `.zip.sha256`；用 `Get-FileHash .\300-Questions-windows-x64.zip -Algorithm SHA256` 核对 ZIP，再完整解压内层 ZIP。内含 `dioxus-app.exe`、中文 `运行说明.txt` 和 exe 的 `SHA256SUMS.txt`。校验和不等于数字签名。
- Windows 需要 **Microsoft Edge WebView2 Evergreen Runtime x64**；缺失时从 [微软官网](https://developer.microsoft.com/microsoft-edge/webview2/) 安装。如提示缺少 VCRUNTIME/MSVCP DLL，安装 [Visual C++ Redistributable x64](https://aka.ms/vs/17/release/vc_redist.x64.exe)。程序无代码签名，SmartScreen 可能警告未知发布者；核实来源后自行决定，不要关闭系统安全保护。发行版不弹出控制台窗口。
- ZIP 只打包 exe、说明与校验和，不含源 Excel、源码、缓存、外置题库或样式。不自动创建公共 Release，不需仓库秘密，workflow 权限仅 `contents: read`。`target/` 和 `dist/` 保持忽略。
- CI 执行 `cargo test --locked -- --skip room::network_tests`；另一步明确说明跳过真实套接字/发现/时序相关的 LAN 集成测试，因为它们不适合作为稳定 CI 验收。没有静默忽略可移植测试失败。本地合适网络上可单独执行 `cargo test --locked room::network_tests -- --test-threads=1`。Windows GUI、WebView2 启动、动画和跨设备局域网仍须实机验证，构建成功不代表这些已验收。

Windows 开发机可安装稳定 Rust 与 Visual Studio C++ Build Tools 后，在项目目录用 PowerShell 7 执行：

```powershell
rustup target add x86_64-pc-windows-msvc
cargo fmt --check
cargo test --locked -- --skip room::network_tests
cargo build --release --locked --target x86_64-pc-windows-msvc
./scripts/package-windows.ps1
```

打包脚本先检查发行 exe 存在且非空，只从指定构建目录取文件，写入 `dist/300-Questions-windows-x64.zip` 和 ZIP 校验和。局域网运行需允许程序 TCP 入站和 UDP 42739，建议只允许可信专用网络；程序不会自动修改防火墙。单人练习无需联网。

## 项目结构

- `src/main.rs`：主页状态、字段、主题和界面切换。
- `src/questions.rs`：内嵌题库解析与结构校验。
- `src/practice.rs`：纯选题、计分和阶段推进逻辑及测试。
- `src/practice_ui.rs`：练习/结果界面、作用域定时任务。
- `assets/main.css`：主页及练习主题、反馈、动画和响应式布局。
- `assets/questions.json`：仅题干、四选项、答案。
- `scripts/extract_questions.py`：只读 XLSX 提取与可选完整 Markdown 审计比较。

开发版程序在 `target/debug/dioxus-app`；发行版可用 `cargo build --release --locked` 构建。这不是独立安装包，仍依赖平台 WebView 与原生库。Linux 需要 GTK 3、WebKitGTK 4.1、libxdo、OpenSSL 等开发/运行库，以及有效 X11/Wayland 图形会话；无图形环境仅能构建和运行纯逻辑测试。当前配置了 Windows x64 ZIP 和 Ubuntu 24.04 基线 Linux x64 tar.gz（均不是安装程序或自包含包），未配置 Web、移动端、热重载或安装器。

Linux 开发机准备开发库后可执行：

```bash
sudo apt-get install -y build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
rustup target add x86_64-unknown-linux-gnu
cargo fmt --check
cargo test --locked -- --skip room::network_tests
cargo build --release --locked --target x86_64-unknown-linux-gnu
bash scripts/package-linux.sh
```

Linux 脚本仅从指定 target 目录读取非空可执行文件，输出 `dist/300-Questions-linux-x64.tar.gz` 及外部 `.sha256`；包内二进制权限为 755，并包含运行说明和 SHA256SUMS.txt。
