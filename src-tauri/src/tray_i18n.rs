//! Pure tray / native-dialog copy. Language parse matches GUI `mapLanguageToUi`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayUiLanguage {
    Zh,
    En,
}

pub(crate) fn parse_tray_language(raw: &str) -> TrayUiLanguage {
    let v = raw.trim().to_ascii_lowercase();
    if v.starts_with("en") {
        TrayUiLanguage::En
    } else {
        TrayUiLanguage::Zh
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrayMenuCopy {
    pub show: &'static str,
    pub open_routes: &'static str,
    pub start_routes: &'static str,
    pub stop_routes: &'static str,
    pub quit: &'static str,
}

pub(crate) fn tray_menu_copy(lang: TrayUiLanguage) -> TrayMenuCopy {
    match lang {
        TrayUiLanguage::Zh => TrayMenuCopy {
            show: "打开 AgentHub",
            open_routes: "打开路由",
            start_routes: "启动路由",
            stop_routes: "停止路由",
            quit: "退出",
        },
        TrayUiLanguage::En => TrayMenuCopy {
            show: "Open AgentHub",
            open_routes: "Open routes",
            start_routes: "Start routes",
            stop_routes: "Stop routes",
            quit: "Quit",
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrayDialogCopy {
    pub running_title: &'static str,
    pub hide_to_tray: &'static str,
    pub stop_and_quit: &'static str,
    pub keep_running: &'static str,
    pub keep_running_ellipsis: &'static str,
    pub cancel: &'static str,
    pub status_unavailable: &'static str,
    pub stop_and_restart: &'static str,
    pub continue_title: &'static str,
    pub hide_body_exit: &'static str,
    pub hide_body_restart: &'static str,
    pub impact_quit: &'static str,
    pub impact_restart: &'static str,
    pub running_count_template: &'static str,
}

impl TrayDialogCopy {
    pub(crate) fn running_count_line(&self, count: usize) -> String {
        self.running_count_template
            .replacen("{count}", &count.to_string(), 1)
    }
}

pub(crate) fn tray_dialog_copy(lang: TrayUiLanguage) -> TrayDialogCopy {
    match lang {
        TrayUiLanguage::Zh => TrayDialogCopy {
            running_title: "本机路由正在运行",
            hide_to_tray: "隐藏到托盘",
            stop_and_quit: "停止服务并退出",
            keep_running: "继续运行",
            keep_running_ellipsis: "继续运行…",
            cancel: "取消",
            status_unavailable: "本机路由状态暂时无法读取。",
            stop_and_restart: "停止服务并重启",
            continue_title: "继续运行本机路由？",
            hide_body_exit:
                "选择“隐藏到托盘”会保留正在运行的本机路由和 Connections；也可以取消本次退出。",
            hide_body_restart:
                "选择“隐藏到托盘”会保留正在运行的本机路由和 Connections，并暂不重启；也可以取消本次重启。",
            impact_quit: "停止服务并退出会中断这些本地 Connections。也可以让它们继续在托盘中运行，或取消本次操作。",
            impact_restart:
                "停止服务并重启会中断这些本地 Connections。也可以让它们继续在托盘中运行，或取消本次操作。",
            running_count_template: "{count} 个本机路由正在运行。",
        },
        TrayUiLanguage::En => TrayDialogCopy {
            running_title: "Local routes running",
            hide_to_tray: "Hide to tray",
            stop_and_quit: "Stop and quit",
            keep_running: "Keep running",
            keep_running_ellipsis: "Keep running…",
            cancel: "Cancel",
            status_unavailable: "Local route status is temporarily unavailable.",
            stop_and_restart: "Stop and restart",
            continue_title: "Keep local routes running?",
            hide_body_exit:
                "\"Hide to tray\" keeps running local routes and Connections. You can also cancel this quit.",
            hide_body_restart:
                "\"Hide to tray\" keeps running local routes and Connections and skips restart. You can also cancel this restart.",
            impact_quit:
                "Stopping and quitting will interrupt these local Connections. You can keep them running in the tray, or cancel.",
            impact_restart:
                "Stopping and restarting will interrupt these local Connections. You can keep them running in the tray, or cancel.",
            running_count_template: "{count} local route(s) running.",
        },
    }
}
