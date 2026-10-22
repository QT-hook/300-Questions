# Run with PowerShell 7 after the locked Windows x64 release build.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target/x86_64-pc-windows-msvc/release/dioxus-app.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw "未找到 Windows x64 发行程序，请先执行 cargo build --release --locked --target x86_64-pc-windows-msvc：$exe"
}
if ((Get-Item -LiteralPath $exe).Length -eq 0) {
    throw '发行程序为空，停止打包。'
}

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
$stage = Join-Path $dist ('windows-stage-' + [guid]::NewGuid().ToString('N'))
$zip = Join-Path $dist '300-Questions-windows-x64.zip'
try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'dioxus-app.exe')
    @'
300 题离线练习 — Windows x64 运行说明

1. 请先完整解压 ZIP，再双击 dioxus-app.exe。适用于 Windows x64，不是安装程序。
2. 需要 Microsoft Edge WebView2 Evergreen Runtime（x64）。Windows 通常已预装；缺失或启动失败时，请从微软官方页面安装：
   https://developer.microsoft.com/microsoft-edge/webview2/
   如提示缺少 VCRUNTIME/MSVCP DLL，请安装微软 Visual C++ Redistributable x64：
   https://aka.ms/vs/17/release/vc_redist.x64.exe
3. 程序未进行代码签名，SmartScreen 可能提示未知发布者。仅在确认下载来源并核对校验和后自行决定是否运行，不要关闭系统安全保护。
4. 单人练习不需要联网；300 道题库和样式已内嵌，无需 Excel、外部 JSON 或源码。
5. 局域网房间仅用于可信网络，不提供 TLS 或身份认证。Windows 防火墙可能要求允许专用网络通信；需允许本程序 TCP 入站及 UDP 42739。程序不会自动修改防火墙。不建议允许公共网络。发现失败可输入正确网卡的 IP:端口，网络客户端隔离会阻断通信。
6. 房间快照保存到 %LOCALAPPDATA%\dioxus-app（缺失时尝试 %USERPROFILE%\AppData\Local；均不可用时才使用临时目录）。每次启动产生新身份，当前没有重启恢复或历史管理入口。
7. SHA256SUMS.txt 为 exe 校验和；ZIP 旁的 .zip.sha256 为 ZIP 校验和。可用 PowerShell Get-FileHash -Algorithm SHA256 核对，校验和不等于发布者签名。
8. CI 构建和可移植单元测试不能证明 GUI、WebView2、动画或跨设备联机已验收；局域网时序测试在 CI 中明确跳过，仍需 Windows 实机确认。
'@ | Set-Content -LiteralPath (Join-Path $stage '运行说明.txt') -Encoding utf8BOM
    $exeHash = (Get-FileHash -LiteralPath (Join-Path $stage 'dioxus-app.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    "$exeHash  dioxus-app.exe" | Set-Content -LiteralPath (Join-Path $stage 'SHA256SUMS.txt') -Encoding ascii
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -Force
    $zipHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$zipHash  300-Questions-windows-x64.zip" | Set-Content -LiteralPath "$zip.sha256" -Encoding ascii
    Write-Output "已生成 $zip"
    Write-Output "SHA256 $zipHash"
}
finally {
    if (Test-Path -LiteralPath $stage) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
}
