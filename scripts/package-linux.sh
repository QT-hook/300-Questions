#!/usr/bin/env bash
# Run after the locked x86_64-unknown-linux-gnu release build on Ubuntu 24.04.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/target/x86_64-unknown-linux-gnu/release/dioxus-app"
if [[ ! -s "$binary" || ! -x "$binary" ]]; then
    printf '%s\n' '未找到非空可执行文件，请先 cargo build --release --locked --target x86_64-unknown-linux-gnu' >&2
    exit 1
fi
mkdir -p -- "$root/dist"
stage="$(mktemp -d "$root/dist/linux-stage-XXXXXXXX")"
trap 'rm -rf -- "$stage"' EXIT
install -m 755 -- "$binary" "$stage/dioxus-app"
cat > "$stage/运行说明.txt" <<'NOTES'
300 题离线练习 — Linux x64 运行说明

1. 解压 tar.gz 后，在有效的 X11/Wayland 桌面会话中运行 ./dioxus-app。不是安装器、AppImage 或自包含程序，不支持无图形环境启动 GUI。
2. 构建基线为 Ubuntu 24.04 x86_64（glibc 2.39）。更旧的 glibc/发行版不保证兼容；其他发行版需要自行确认 ABI 和运行库兼容性。
3. Ubuntu 24.04 可安装运行依赖：
   sudo apt update
   sudo apt install libgtk-3-0t64 libwebkit2gtk-4.1-0 libjavascriptcoregtk-4.1-0 libxdo3 libssl3t64 libayatana-appindicator3-1 librsvg2-2 xdg-utils fonts-noto-cjk
   这些原生库不包含在压缩包中。xdg-utils 用于在系统浏览器打开 GitHub 链接；中文显示为方框时请安装 CJK 字体（如 fonts-noto-cjk）。
4. 300 道题库和样式已内嵌，单人练习无需联网，无需 Excel、外部 JSON 或源码。
5. 局域网仅用于可信网络，无 TLS 或身份认证。需允许程序 TCP 入站及 UDP 42739；不会自动修改防火墙。发现失败可手动输入正确网卡 IP:端口，客户端隔离可能阻断联机。
6. 房间快照存储在 $XDG_DATA_HOME/dioxus-app，未设置时为 ~/.local/share/dioxus-app；相关环境变量不可用时回退临时目录。重启产生新身份，目前无自动存档恢复入口。
7. 可在解压目录执行 sha256sum -c SHA256SUMS.txt 校验二进制；下载的 .tar.gz.sha256 校验压缩包。校验和不是发布者签名，程序未签名。
8. CI 构建与可移植测试不代表 GUI、动画、外部链接或跨机器局域网已验收。room::network_tests 的真实套接字/发现/时序测试在 CI 明确跳过，仍需实机验证。
NOTES
(cd -- "$stage" && sha256sum dioxus-app > SHA256SUMS.txt)
archive='300-Questions-linux-x64.tar.gz'
tar -czf "$root/dist/$archive" -C "$stage" dioxus-app 运行说明.txt SHA256SUMS.txt
(cd -- "$root/dist" && sha256sum "$archive" > "$archive.sha256")
printf '已生成 %s\n' "$root/dist/$archive"
