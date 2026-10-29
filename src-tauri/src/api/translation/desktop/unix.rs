use super::*;
use crate::core::desktop;

pub fn start(app: AppHandle) {
    desktop::start_shortcuts(
        app,
        |app| {
            let service = app.state::<DesktopTranslator>();
            let state = service.inner.lock().unwrap();
            if !state.enabled {
                return Vec::new();
            }
            vec![
                ("selection".into(), state.shortcut.clone()),
                ("clipboard".into(), state.clipboard_shortcut.clone()),
            ]
        },
        |app, action| {
            let app = app.clone();
            let clipboard = action == "clipboard";
            std::thread::spawn(move || {
                let position = desktop::physical_cursor_position(&app)
                    .ok()
                    .or_else(|| {
                        app.primary_monitor()
                            .ok()
                            .flatten()
                            .map(|monitor| (monitor.position().x + 80, monitor.position().y + 80))
                    })
                    .unwrap_or((80, 80));
                // 等待用户松开全局快捷键修饰键，避免与复制操作叠加。
                std::thread::sleep(std::time::Duration::from_millis(200));
                let text = if clipboard {
                    desktop::clipboard_text()
                } else {
                    desktop::selected_text()
                }
                .map_err(|error| error.to_string());
                show_selection(&app, text, position.0, position.1);
            });
        },
        |app, errors| {
            app.state::<DesktopTranslator>()
                .inner
                .lock()
                .unwrap()
                .hotkey_error = errors.join("；");
        },
    );
}
