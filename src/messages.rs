use std::borrow::Cow;
/// Translate app-owned errors while retaining OS/backend detail verbatim.
pub fn display(message: &str, chinese: bool) -> Cow<'_, str> {
    if !chinese {
        return Cow::Borrowed(message);
    }
    let pairs = [
        ("Multiple connection profiles match this display. Choose a profile explicitly.", "此显示器有多个连接配置，请明确选择。"),
        ("Unknown saved connection profile.", "找不到已保存的连接配置。"),
        ("Profile and display selectors disagree.", "连接配置与显示器的选择不一致。"),
        ("Finish or suspend setup before choosing a profile.", "请先完成设置或选择稍后完成，再选择连接配置。"),
        ("Remembered profile choice is missing or belongs to another display.", "记住的连接配置不存在或属于另一台显示器。"),
        ("Duplicate or invalid connection profile identity", "连接配置标识重复或无效"),
        ("The selected display identity is ambiguous. No display will be controlled.", "所选显示器标识不明确，不会控制任何显示器。"),
        ("Workspace saved; completed draft cleanup needs attention. Quit PortKey, then remove setup-draft-rust.json from the configuration directory.", "工作区已保存，但已完成的草稿无法清理。请退出 PortKey，然后删除配置目录中的 setup-draft-rust.json。"),
        ("A completed setup draft remains on disk and cannot be resumed. Quit PortKey, then remove setup-draft-rust.json from the configuration directory.", "磁盘上残留了已完成的设置草稿，无法继续该草稿。请退出 PortKey，然后删除配置目录中的 setup-draft-rust.json。"),
        ("This setup draft was already saved and cannot be resumed. Start a new setup to make changes.", "此设置草稿已经保存，无法继续。请新建设置以进行更改。"),
        ("Give the two computers distinguishable names.", "请为两台电脑设置不同的名称。"),
        ("Names must contain 1 to 80 printable characters.", "名称须包含 1 至 80 个可显示字符。"),
        ("Keep USB on this computer, then start USB detection.", "请先将 USB 切换到本机，再开始检测 USB 开关。"),
        ("Monitor selected. Keep USB on this computer, then start USB detection.", "已选择显示器。请将 USB 切换到本机，再开始检测 USB 开关。"),
        ("Workspace must be an object.", "工作区数据格式无效。"),
        ("Qualification binding cannot be supplied by the frontend.", "界面不能直接指定硬件验证绑定。"),
        ("Historical provenance is not supported for new qualification.", "新版配置不支持使用旧版历史记录作为验证。"),
        ("Invalid workspace:", "工作区数据无效："),
        ("Invalid computer, installation mode or qualification choice.", "电脑、安装方式或验证选项无效。"),
        ("Invalid workspace identity or revision.", "工作区标识或版本无效。"),
        ("Invalid directional evidence or provenance.", "切换方向的证据或来源无效。"),
        ("Both directions require explicit observed evidence and computer OS choices.", "两个切换方向都需要明确的观察记录，并选择两台电脑的操作系统。"),
        ("Single-controller qualification requires that the other instance was paused or quit.", "验证单机控制前，须确认另一台电脑上的 PortKey 已暂停或退出。"),
        ("Dual qualification requires confirmed reciprocal enrollment.", "验证双机控制前，须确认两台电脑的对应配置。"),
        ("Saved connections retained. Automatic switching stays paused until you finish editing.", "已保留原有连接；完成编辑前，自动切换保持暂停。"),
        ("Test cancelled because power state changed. Start the test again while awake.", "电源状态已变化，测试已取消。请在电脑唤醒后重新开始。"),
        ("The setup changed. Start the test again.", "设置已变化，请重新开始测试。"),
        ("Reconnect the profile's display before testing.", "请先连接此配置绑定的显示器，再开始测试。"),
        ("Finish or discard the existing setup draft first.", "请先完成或放弃现有的设置草稿。"),
        ("Start setup first.", "请先开始设置。"),
        ("Choose a saved workspace first.", "请先选择已保存的工作区。"),
        ("Unknown saved workspace.", "找不到已保存的工作区。"),
        ("Qualify this controller and both directions before enabling automation.", "请先验证此控制电脑及两个切换方向，再启用自动切换。"),
        ("Qualify both directions before enabling automation.", "请先验证两个切换方向，再启用自动切换。"),
        ("Keep USB on this computer while PortKey captures a fresh baseline. Wait for the next instruction before switching.", "PortKey 正在采集新的 USB 基准，请保持 USB 连接本机，等待下一步提示后再切换。"),
        ("No USB hubs detected. Connect the switch to this computer first.", "未检测到 USB 集线器，请先将 USB 开关连接到本机。"),
        ("USB topology is ambiguous; reconnect and retry.", "无法明确识别 USB 拓扑，请重新连接后重试。"),
        ("Switch USB to the other computer and keep it there until PortKey confirms departure. Detection may take 15 seconds. Keep the monitor input unchanged.", "请将 USB 切换到另一台电脑，并保持到 PortKey 确认断开。检测可能需要 15 秒，请保持显示器输入不变。"),
        ("USB detection timed out. Return USB to this computer, retry detection, and wait for each on-screen instruction.", "USB 检测超时。请切回本机后重新检测，并等待每一步屏幕提示。"),
        ("More than two hubs changed. Disconnect other changing hubs and restart USB detection.", "发生变化的集线器超过两个，请断开其他变化的集线器后重新检测。"),
        ("USB returned before departure could be confirmed. Keep USB here, retry detection, then wait for PortKey to ask you to switch back.", "尚未确认断开，USB 就已返回。请保持连接本机并重试，等待 PortKey 提示后再切回。"),
        ("USB departure detected. Switch USB back to this computer and wait.", "已检测到 USB 断开。请将 USB 切回本机并等待。"),
        ("USB switch detected. Show this computer on the selected monitor, then read its input.", "已识别 USB 开关。请在所选显示器上显示本机画面，再读取输入。"),
        ("Both computers use the same input. Select the other picture and retry.", "两台电脑的输入相同，请选择另一台电脑的画面后重试。"),
        ("Copy this code to the other computer. Enter the code shown there to finish.", "请将此设置记录复制到另一台电脑，并导入对方的记录以完成映射。"),
        ("The monitor returned an invalid input. Show this computer on the monitor and retry.", "显示器返回了无效输入。请显示本机画面后重试。"),
        ("Read this computer's monitor input first.", "请先读取本机的显示器输入。"),
        ("Read this computer's input first.", "请先读取本机输入。"),
        ("Both computers reported the same input. Show each computer on its own monitor input and read again.", "两台电脑回报了相同输入。请分别显示各自画面并重新读取。"),
        ("Ready to save. Automatic switching will stay paused until you enable it.", "可以保存了。自动切换将保持暂停，直到您明确启用。"),
        ("Discovery or sleep interrupted setup. Restart USB detection.", "设备发现或睡眠中断了设置，请重新检测 USB 开关。"),
        ("Complete USB detection and exchange both computer codes first.", "请先完成 USB 检测及两台电脑的输入映射。"),
        ("Setup record is too large.", "设置记录过大。"),
        ("Invalid setup record; copy the complete version 2 record.", "设置记录无效，请复制完整的第 2 版记录。"),
        ("Invalid setup record.", "设置记录无效。"),
        ("Invalid or incompatible setup record.", "设置记录无效或版本不兼容。"),
        ("Workspace record adopted. Select and enroll this computer's monitor and USB switch, then read its input and preview the peer record again.", "已采用工作区记录。请在本机选择并配置显示器及 USB 开关，读取输入后再次预览对方记录。"),
        ("Complete USB detection first.", "请先完成 USB 检测。"),
        ("Wrong workspace, incompatible or stale setup record. Review workspace identity and revision.", "设置记录属于其他工作区、版本不兼容或已过期，请检查工作区标识及版本。"),
        ("The record claims this computer's role. Choose the other role.", "此记录使用了本机的角色，请选择另一角色。"),
        ("Computer names or OS choices disagree. Review both workspace records.", "电脑名称或操作系统不一致，请检查两台电脑的工作区记录。"),
        ("Reciprocal input mappings disagree. Review each computer's picture and replace stale records.", "两台电脑的对应输入不一致，请检查各自画面并替换过期记录。"),
        ("Preview the other computer's setup record first.", "请先预览另一台电脑的设置记录。"),
        ("Invalid setup code. Copy the complete PK1 code from the other computer.", "设置代码无效，请复制另一台电脑的完整 PK1 代码。"),
        ("Selected display disconnected or became ambiguous. Reconnect it and restart USB detection.", "所选显示器已断开或无法明确识别，请重新连接并重新检测 USB 开关。"),
        ("Wait for display discovery first.", "请先等待显示器发现完成。"),
        ("Choose one connected display first.", "请先明确选择一台已连接的显示器。"),
        ("Enter a desk name from 1 to 80 characters.", "请输入 1 至 80 个字符的工作区名称。"),
        ("Restore power monitoring and wake this computer before setup.", "请先恢复电源监控并唤醒本机，再进行设置。"),
        ("Wait for fresh USB discovery before starting detection.", "请先等待新的 USB 发现结果，再开始检测。"),
        ("Wait for USB discovery first.", "请先等待 USB 发现完成。"),
        ("Choose a display first.", "请先选择显示器。"),
        ("Reconnect the selected display before starting USB detection.", "请先重新连接所选显示器，再检测 USB 开关。"),
        ("Restore power monitoring and wake this computer before reading its input.", "请先恢复电源监控并唤醒本机，再读取输入。"),
        ("Reading the selected monitor's current input…", "正在读取所选显示器的当前输入…"),
        ("Finish setup and restore power monitoring before enabling automation.", "请先完成设置并恢复电源监控，再启用自动切换。"),
        ("Switching unavailable while editing, sleeping or power monitoring is unavailable.", "正在编辑、睡眠中或电源监控不可用时，无法切换。"),
        ("Complete enrollment first.", "请先完成设备配置。"),
        ("PortKey is shutting down.", "PortKey 正在退出。"),
        ("PortKey is already running", "PortKey 已在运行"),
        ("Configuration belongs to another platform; enroll this computer.", "配置属于另一操作系统，请在本机重新配置。"),
        ("Draft path needs a parent directory", "草稿路径需要父目录"),
        ("Cannot create draft directory:", "无法创建草稿目录："),
        ("Cannot save draft:", "无法保存草稿："),
        ("Cannot replace draft:", "无法替换草稿："),
        ("Cannot read setup draft:", "无法读取设置草稿："),
        ("Setup draft is too large.", "设置草稿过大。"),
        ("Invalid setup draft:", "设置草稿无效："),
        ("Cannot remove setup draft:", "无法删除设置草稿："),
        ("PORTKEY_CONFIG_DIR must be an absolute directory without parent traversal.", "PORTKEY_CONFIG_DIR 必须是绝对目录，且不能包含上级目录跳转。"),
        ("Cannot preserve original profiles:", "无法保留原始配置："),
        ("Invalid profile store:", "配置存储无效："),
        ("Cannot read profile store:", "无法读取配置存储："),
        ("Cannot create profile directory:", "无法创建配置目录："),
        ("Cannot create profile file:", "无法创建配置文件："),
        ("Cannot write profiles:", "无法写入配置："),
        ("Cannot replace profiles:", "无法替换配置："),
        ("Unsupported profile store version", "不支持此配置存储版本"),
        ("Duplicate display profile identity", "显示器配置标识重复"),
        ("Profile belongs to another platform; enroll this computer.", "此配置属于另一操作系统，请在本机配置设备。"),
        ("Profile name must contain printable characters", "工作区名称须包含可显示字符"),
        ("Multiple enrolled displays are connected; disconnect all but one before switching.", "同时连接了多台已配置的显示器，请仅保留一台后再切换。"),
        (
            "Choose a display with a stable platform identity",
            "请选择具有稳定标识的显示器",
        ),
        (
            "Local and remote inputs must be distinct numbers from 1 to 255",
            "本机与另一台电脑的输入值须为 1 至 255 之间的不同数字",
        ),
        (
            "Enroll two different shared hubs",
            "请选择两个不同的共享 USB 集线器",
        ),
        (
            "Each hub requires a complete stable USB topology; re-enroll after moving ports",
            "每个集线器都需要完整稳定的 USB 拓扑，移动接口后请重新配置",
        ),
        (
            "Choose hubs at two different USB locations",
            "请选择两个不同 USB 位置的集线器",
        ),
        ("Unsupported language", "不支持的语言"),
        (
            "Configuration requires macos or windows platform",
            "配置仅支持 macOS 或 Windows 平台",
        ),
        ("Cannot read configuration:", "无法读取配置："),
        ("Invalid configuration JSON:", "配置文件格式错误："),
        (
            "Cannot create configuration directory:",
            "无法创建配置目录：",
        ),
        (
            "Cannot create temporary configuration:",
            "无法创建临时配置：",
        ),
        ("Cannot write configuration:", "无法写入配置："),
        ("Cannot replace configuration:", "无法替换配置："),
        (
            "Request expired or was cancelled; retry after discovery.",
            "请求已过期或取消，请在设备发现完成后重试。",
        ),
        (
            "USB ownership changed; automatic request cancelled.",
            "USB 归属已改变，自动请求已取消。",
        ),
        (
            "Request cancelled by lifecycle or newer action.",
            "请求因睡眠状态变化或新操作而取消。",
        ),
        (
            "USB ownership changed; retry cancelled.",
            "USB 归属已改变，重试已取消。",
        ),
        ("Request cancelled before retry.", "请求在重试前已取消。"),
        (
            "Sleep interrupted the operation; outcome is unknown.",
            "睡眠中断了操作，结果未知。",
        ),
    ];
    for (en, zh) in pairs {
        if let Some(detail) = message.strip_prefix(en) {
            return Cow::Owned(format!("{zh}{detail}"));
        }
    }
    Cow::Borrowed(message)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changes_language_without_losing_system_detail() {
        assert_eq!(
            display("Enroll two different shared hubs", true),
            "请选择两个不同的共享 USB 集线器"
        );
        assert_eq!(
            display("Cannot read configuration: permission denied", true),
            "无法读取配置： permission denied"
        );
        assert_eq!(display("native driver 42", true), "native driver 42");
    }
}
