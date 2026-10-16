# Portkey

**Portkey（门钥匙）**，名字来自 Harry Potter：像门钥匙把人送到另一处，Portkey 把键鼠所在的主机与屏幕输入连在一起。

原生 macOS 菜单栏 app：根据已绑定的共享 USB2 / USB3 Hub，自动把 Philips 27M2N5800 切到 Mac 的 HDMI1（17）或 Windows 的 DisplayPort（15）。Mac 和 Windows 均需保持唤醒；Windows 无需安装软件。

## 构建与运行

需要 Apple Silicon Mac、macOS 13+ 和 Apple Command Line Tools（`xcode-select --install`）。无第三方 Swift 依赖、无管理员后台服务。

```sh
cd /Users/jabriel/dev/usb-follow-display
./scripts/test.sh
./scripts/build-app.sh
open 'dist/Portkey.app'
```

构建会编译固定版本的 m1ddc、保留许可证、生成并本地 ad-hoc 签名 `.app`。此开发包尚未公证。默认使用已实测的 Philips UUID 和双 Hub 拓扑。菜单栏显示一个显示器图标；M 表示 Mac 双 Hub 齐全，? 表示双 Hub 不在 Mac，· 表示状态不完整，⏸ 表示暂停。

菜单提供当前 USB/输入状态、自动开关、手动 Mac/Windows、设置/重新绑定、日志和退出。手动切换保持到下一次真正的 USB 所有权变化；自动暂停时仍可手动切换。读回确认仅表示 DDC 返回目标输入，不等于用户确认了实际画面。

## 绑定与恢复

在“设置 / 重新绑定”检查在线显示器 UUID、共享 USB2/USB3 Hub 的 location 和父 Hub。相同型号的上游 Hub 会同时存在，不能仅按 VID/PID 绑定。更换 Mac 端口、扩展坞或线缆后，请先把 USB 切回 Mac，然后明确选择新的共享分支并保存。无效编辑不会覆盖原配置；“重新扫描 / 撤销编辑”恢复已保存值。

为保留已有绑定和开关状态，配置目录与 bundle identifier 沿用早期名称。配置：`~/Library/Application Support/USBFollowDisplay/config.json`。自动开关通过 app 的本地偏好保存。日志：`~/Library/Logs/USBFollowDisplay/app.log`（约 512 KB 轮换一次）。不记录键盘内容、鼠标动作或网络信息。

启动时 USB 不在 Mac，只显示未知归属，不会据此切到 Windows。双 Hub 部分到达时等待；键盘单独拔出不会切屏。睡眠取消待执行操作，唤醒后重新枚举；USB 不在 Mac 时不补发旧离开命令。当前实测可在锁屏时恢复外接屏，无需辅助功能权限；但显示器和系统重新就绪有延迟，本机观察到约 23 秒后读回确认，不能保证立即恢复。完整拔掉或断电 USB 切换器与切到 Windows 在物理信号上无法区分。

写入始终明确指定显示器 UUID。每个请求最多尝试 3 次，失败间隔 750/1500 ms；每个 helper 进程有 2.5 s 上限，并会被终止和回收。此 Philips 切换后最初数秒可能读回 0，所以每次写入后等待 4 s 再读回；取消或反转会立即打断等待。0、乱码、非已绑定输入均显示未确认，不会声称画面切换成功。

可从终端只读诊断（不会切屏）：

```sh
'dist/Portkey.app/Contents/MacOS/Portkey' --diagnose
```

重新打开已运行的 app 会显示设置窗口；设置中的“打开状态菜单”也能访问同一菜单。`--settings` 启动时显示设置窗口；`--paused` 本次启动先暂停自动切换。缺少 helper 时重新构建 app；配置损坏时打开设置重新保存。退出 app 后删除 `.app` 即可停止使用；可另行删除上述配置、日志目录以及 `local.usbfollowdisplay.app` 偏好。没有安装登录启动项或全局 helper。

## 测试与证据

`./scripts/test.sh` 运行可重复的 Swift 状态机、串行调度、真实进程超时/输出和配置测试。当前机器仅有 Command Line Tools，不含 XCTest/Swift Testing，因此使用 SwiftPM executable 断言测试器；`swift test` 在此环境不可用。测试不执行 DDC 写入。

实机结果与待测项目见 [diagnostics/app-acceptance.md](diagnostics/app-acceptance.md)；此前底层 DDC 与 USB 拓扑证据分别见 [DDC findings](diagnostics/ddc-findings.md) 和 [USB findings](diagnostics/usb-switch-findings.md)。模拟生命周期测试不能替代真实睡眠验收。

后端来源、固定 revision 和许可证见 [ThirdParty/m1ddc/UPSTREAM.md](ThirdParty/m1ddc/UPSTREAM.md)。没有推送或发布。
