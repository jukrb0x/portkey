use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Pause,
    Local,
    Remote,
    Quit,
}
pub struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    pause: MenuItem,
    local: MenuItem,
    remote: MenuItem,
    quit: MenuItem,
}
impl Tray {
    pub fn new(context: eframe::egui::Context) -> Result<Self, String> {
        let menu = Menu::new();
        let open = MenuItem::with_id("open", "Open PortKey", true, None);
        let pause = MenuItem::with_id("pause", "Pause automatic switching", true, None);
        let local = MenuItem::with_id("local", "Use this computer", false, None);
        let remote = MenuItem::with_id("remote", "Use other computer", false, None);
        let quit = MenuItem::with_id("quit", "Quit PortKey", true, None);
        menu.append_items(&[&open, &pause, &local, &remote, &quit])
            .map_err(|e| e.to_string())?;
        let mut rgba = vec![0; 32 * 32 * 4];
        for y in 4..28 {
            for x in 4..28 {
                let border = !(7..25).contains(&x) || !(7..25).contains(&y);
                let stem = (11..15).contains(&x) && (10..23).contains(&y);
                let bowl =
                    (14..22).contains(&x) && ((10..14).contains(&y) || (16..19).contains(&y));
                if border || stem || bowl {
                    let p = (y * 32 + x) * 4;
                    rgba[p..p + 4].copy_from_slice(&[80, 190, 220, 255]);
                }
            }
        }
        let icon = Icon::from_rgba(rgba, 32, 32).map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("PortKey")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| e.to_string())?;
        // Wake egui even when its settings window is hidden.
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            EVENTS.lock().unwrap().push(event.id.0);
            context.request_repaint();
        }));
        Ok(Self {
            _icon: tray,
            open,
            pause,
            local,
            remote,
            quit,
        })
    }
    pub fn update(&self, chinese: bool, enrolled: bool) {
        self.open.set_text(if chinese {
            "打开 PortKey"
        } else {
            "Open PortKey"
        });
        self.pause.set_text(if chinese {
            "暂停自动切换"
        } else {
            "Pause automatic switching"
        });
        self.local.set_text(if chinese {
            "切换到本机"
        } else {
            "Use this computer"
        });
        self.remote.set_text(if chinese {
            "切换到另一台电脑"
        } else {
            "Use other computer"
        });
        self.quit.set_text(if chinese {
            "退出 PortKey"
        } else {
            "Quit PortKey"
        });
        self.local.set_enabled(enrolled);
        self.remote.set_enabled(enrolled);
    }
    pub fn actions(&self) -> Vec<Action> {
        EVENTS
            .lock()
            .unwrap()
            .drain(..)
            .filter_map(|id| action(&id))
            .collect()
    }
}
static EVENTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
fn action(id: &str) -> Option<Action> {
    match id {
        "open" => Some(Action::Open),
        "pause" => Some(Action::Pause),
        "local" => Some(Action::Local),
        "remote" => Some(Action::Remote),
        "quit" => Some(Action::Quit),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_menu_ids_dispatch_actions() {
        assert_eq!(action("local"), Some(Action::Local));
        assert_eq!(action("pause"), Some(Action::Pause));
        assert_eq!(action("unrelated"), None);
    }
}
