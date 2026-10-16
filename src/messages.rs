use std::borrow::Cow;
/// Translate app-owned errors while retaining OS/backend detail verbatim.
pub fn display(message: &str, chinese: bool) -> Cow<'_, str> {
    if !chinese {
        return Cow::Borrowed(message);
    }
    let pairs = [
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
        ("Cannot read legacy configuration:", "无法读取旧版配置："),
        ("Invalid legacy configuration:", "旧版配置无效："),
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
