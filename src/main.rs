#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod practice;
mod practice_ui;
mod questions;
mod room;

use room::{
    model::Room,
    protocol::Advert,
    service::{Command, Event, Service},
    ui::RoomScreen,
};
use std::time::Duration;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use practice::Selection;
use practice_ui::PracticeScreen;
use std::rc::Rc;

const STYLES: &str = include_str!("../assets/main.css");

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Practice,
    Join,
    Create,
}

#[derive(Clone, Copy, PartialEq)]
enum PracticeMode {
    Fixed,
    Random,
}

fn toggle_panel(current: Option<Panel>, clicked: Panel) -> Option<Panel> {
    if current == Some(clicked) {
        None
    } else {
        Some(clicked)
    }
}

fn positive_integer(value: &str) -> Result<u64, &'static str> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("请输入正整数");
    }
    let number = value.parse::<u64>().map_err(|_| "数值过大")?;
    if number == 0 {
        return Err("请输入大于 0 的整数");
    }
    Ok(number)
}

fn range_end_error(start: &str, end: &str) -> Option<&'static str> {
    let end = match positive_integer(end) {
        Ok(end) => end,
        Err(error) => return Some(error),
    };
    if positive_integer(start).is_ok_and(|start| start > end) {
        Some("结束题号不能小于起始题号")
    } else {
        None
    }
}

fn filter_room_code(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).take(4).collect()
}

fn room_code_error(value: &str) -> Option<&'static str> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        Some("请输入 4 位数字房间号")
    } else {
        None
    }
}

fn room_name_error(value: &str) -> Option<&'static str> {
    if (2..=6).contains(&value.trim().chars().count()) {
        None
    } else {
        Some("房间名称须为 2～6 个字符")
    }
}

fn player_name_error(value: &str) -> Option<&'static str> {
    if (1..=9).contains(&value.trim().chars().count()) {
        None
    } else {
        Some("名字须为 1～9 个字符")
    }
}

#[component]
fn TextField(
    id: &'static str,
    label: &'static str,
    mut value: Signal<String>,
    mut touched: Signal<bool>,
    error: Option<&'static str>,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] numeric: bool,
    #[props(default = false)] code: bool,
    #[props(default = false)] hide_label: bool,
    action_label: Option<&'static str>,
) -> Element {
    let visible_error = if touched() && !disabled { error } else { None };
    let error_id = format!("{id}-error");
    rsx! {
        div { class: "field",
            label {
                class: if hide_label { "field-label visually-hidden" } else { "field-label" },
                r#for: id, "{label}"
            }
            div { class: "input-row",
                input {
                class: "text-input", id, r#type: "text",
                value: value(), disabled, required: true,
                inputmode: if numeric { "numeric" } else { "text" },
                maxlength: if code { Some(4) } else { None },
                aria_invalid: visible_error.is_some(),
                aria_describedby: visible_error.map(|_| error_id.clone()),
                oninput: move |event| {
                    let input = event.value();
                    value.set(if code { filter_room_code(&input) } else { input });
                },
                onblur: move |_| touched.set(true),
                }
                if let Some(text) = action_label {
                    button { class: "primary-button", r#type: "button", "{text}" }
                }
            }
            if let Some(message) = visible_error {
                span { class: "field-error", id: error_id.clone(), aria_live: "polite", "{message}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_integers_reject_invalid_input() {
        for input in ["", "0", "000", "-1", "+1", "1.5", " 1", "1 ", "１２", "一"] {
            assert!(positive_integer(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn positive_integers_accept_values_above_300_and_u64_max() {
        assert_eq!(positive_integer("301"), Ok(301));
        assert_eq!(positive_integer("18446744073709551615"), Ok(u64::MAX));
        assert_eq!(positive_integer("001"), Ok(1));
    }

    #[test]
    fn positive_integers_handle_overflow() {
        assert_eq!(positive_integer("18446744073709551616"), Err("数值过大"));
    }

    #[test]
    fn ranges_require_ordered_positive_endpoints() {
        assert!(range_end_error("10", "9").is_some());
        assert_eq!(range_end_error("10", "10"), None);
        assert_eq!(range_end_error("1", "999"), None);
        assert!(range_end_error("1", "0").is_some());
    }

    #[test]
    fn room_codes_filter_ascii_digits_and_keep_leading_zeroes() {
        assert_eq!(filter_room_code("a0１2-0345"), "0203");
        assert_eq!(filter_room_code("0001"), "0001");
    }

    #[test]
    fn room_codes_require_exactly_four_ascii_digits() {
        assert_eq!(room_code_error("0001"), None);
        for input in ["", "123", "12345", "１２３４", "abcd"] {
            assert!(room_code_error(input).is_some(), "accepted {input:?}");
        }
    }

    #[test]
    fn room_names_count_trimmed_unicode_characters() {
        for name in ["中文", "  中文  ", "六个中文字符", "ab"] {
            assert_eq!(room_name_error(name), None, "rejected {name:?}");
        }
        for name in ["", "   ", "中", "七个中文字符啊"] {
            assert!(room_name_error(name).is_some(), "accepted {name:?}");
        }
    }

    #[test]
    fn player_names_accept_one_to_nine_trimmed_characters() {
        for name in ["中", "abcdefghi", "一二三四五六七八九", "  小明  "] {
            assert_eq!(player_name_error(name), None, "rejected {name:?}");
        }
    }

    #[test]
    fn player_names_reject_empty_or_over_nine_characters() {
        for name in ["", "   ", "abcdefghij", "一二三四五六七八九十"] {
            assert!(player_name_error(name).is_some(), "accepted {name:?}");
        }
    }

    #[test]
    fn panels_toggle_and_replace_the_current_panel() {
        assert!(toggle_panel(None, Panel::Practice) == Some(Panel::Practice));
        assert!(toggle_panel(Some(Panel::Practice), Panel::Practice).is_none());
        assert!(toggle_panel(Some(Panel::Practice), Panel::Join) == Some(Panel::Join));
    }
}

fn main() {
    let window = WindowBuilder::new()
        .with_title("Dioxus")
        .with_inner_size(LogicalSize::new(960.0, 720.0))
        .with_min_inner_size(LogicalSize::new(560.0, 520.0));

    LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}

#[component]
fn App() -> Element {
    let service = use_hook(|| Service::new().map(Rc::new));
    let mut room = use_signal(|| None::<Room>);
    let mut adverts = use_signal(Vec::<Advert>::new);
    let mut status = use_signal(String::new);
    let mut network_error = use_signal(|| None::<String>);
    let mut manual_endpoint = use_signal(String::new);
    let creator_name = use_signal(String::new);
    let mut creator_touched = use_signal(|| false);
    let poll_service = service.clone();
    use_future(move || {
        let service = poll_service.clone();
        async move {
            let Ok(service) = service else {
                return;
            };
            loop {
                for event in service.poll() {
                    match event {
                        Event::Rooms(found) => adverts.set(found),
                        Event::State(state) => {
                            room.set(Some(*state));
                            network_error.set(None);
                        }
                        Event::Status(message) => status.set(message),
                        Event::Error(message) => network_error.set(Some(message)),
                        Event::Left => room.set(None),
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    });
    let discover_service = service.clone();
    let create_service = service.clone();
    let manual_service = service.clone();
    let mut dark_mode = use_signal(|| true);
    let mut expanded = use_signal(|| None::<Panel>);
    let mut practice_mode = use_signal(|| PracticeMode::Fixed);
    let range_start = use_signal(String::new);
    let range_end = use_signal(String::new);
    let random_count = use_signal(String::new);
    let join_code = use_signal(String::new);
    let player_name = use_signal(String::new);
    let mut player_name_touched = use_signal(|| false);
    let create_code = use_signal(String::new);
    let room_name = use_signal(String::new);
    let mut start_touched = use_signal(|| false);
    let mut end_touched = use_signal(|| false);
    let mut count_touched = use_signal(|| false);
    let mut join_touched = use_signal(|| false);
    let mut create_touched = use_signal(|| false);
    let mut name_touched = use_signal(|| false);
    let bank = use_hook(|| questions::embedded().map(Rc::new));
    let mut active = use_signal(|| None::<Selection>);
    let mut launch_error = use_signal(|| None::<String>);
    let size = bank.as_ref().map_or(0, |bank| bank.len());
    let fixed = practice_mode() == PracticeMode::Fixed;
    let start_error = bounded_integer(&range_start.read(), size).err();
    let end_error = range_end_error(&range_start.read(), &range_end.read())
        .or_else(|| bounded_integer(&range_end.read(), size).err());
    let theme = if dark_mode() { "dark" } else { "light" };
    let toggle_label = if dark_mode() {
        "切换到明亮主题"
    } else {
        "切换到黑暗主题"
    };

    rsx! {
        style { "{STYLES}" }
        main { class: "app", "data-theme": theme,
            button {
                class: "theme-toggle",
                r#type: "button",
                aria_label: toggle_label,
                aria_pressed: dark_mode(),
                onclick: move |_| dark_mode.toggle(),
                svg {
                    width: "22",
                    height: "22",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.6",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    "aria-hidden": "true",
                    if dark_mode() {
                        circle { cx: "12", cy: "12", r: "4" }
                        path { d: "M12 2v2m0 16v2M2 12h2m16 0h2M4.93 4.93l1.42 1.42m11.3 11.3 1.42 1.42M4.93 19.07l1.42-1.42m11.3-11.3 1.42-1.42" }
                    } else {
                        path { d: "M20.9 13.1A9 9 0 0 1 10.9 3.1a9 9 0 1 0 10 10Z" }
                    }
                }
            }
            div { class: "app-content",
            if let Err(error) = &service { p { class: "field-error network-status", role: "alert", "网络服务不可用：{error}" } }
            if !status().is_empty() { p { class: "network-status", role: "status", aria_live: "polite", "{status}" } }
            if let Some(message) = network_error() { p { class: "field-error network-status", role: "alert", "{message}" } }
            if let Err(error) = &bank {
                section { class: "practice-shell", role: "alert", "题库加载失败：{error}" }
            } else if let (Some(current_room), Ok(service), Ok(bank)) = (room(), &service, &bank) {
                RoomScreen { key: "{current_room.id}", service: service.clone(), room: current_room, bank: bank.clone(), onleave: move |_| room.set(None) }
            } else if let Some(selection) = active() {
                if let Ok(bank) = &bank {
                    PracticeScreen { bank: bank.clone(), selection, onhome: move |_| active.set(None) }
                }
            } else {
            nav { class: "menu", aria_label: "主菜单",
                div { class: "menu-entry",
                    button {
                        class: "menu-button", r#type: "button",
                        aria_expanded: expanded() == Some(Panel::Practice),
                        aria_controls: "practice-panel",
                        onclick: move |_| expanded.set(toggle_panel(expanded(), Panel::Practice)),
                        "练习"
                    }
                    div {
                        class: "panel-collapse",
                        "data-open": expanded() == Some(Panel::Practice),
                        aria_hidden: expanded() != Some(Panel::Practice),
                        "inert": if expanded() != Some(Panel::Practice) { Some("") } else { None },
                        div { class: "panel-collapse-inner",
                        section { class: "input-panel", id: "practice-panel", aria_label: "练习设置",
                            div { class: "mode-group", role: "group", aria_label: "练习模式",
                                label { class: "mode-option",
                                    input {
                                        r#type: "radio", name: "practice-mode", value: "fixed",
                                        checked: fixed,
                                        onchange: move |_| practice_mode.set(PracticeMode::Fixed),
                                    }
                                    "固定范围"
                                }
                                div { class: "range-inputs",
                                    TextField {
                                        id: "range-start", label: "起始题号", value: range_start,
                                        touched: start_touched, disabled: !fixed, numeric: true, hide_label: true,
                                        error: start_error,
                                    }
                                    span { class: "range-separator", aria_hidden: "true", "～" }
                                    TextField {
                                        id: "range-end", label: "结束题号", value: range_end,
                                        touched: end_touched, disabled: !fixed, numeric: true, hide_label: true,
                                        error: end_error,
                                    }
                                }
                                label { class: "mode-option",
                                    input {
                                        r#type: "radio", name: "practice-mode", value: "random",
                                        checked: !fixed,
                                        onchange: move |_| practice_mode.set(PracticeMode::Random),
                                    }
                                    "随机模式"
                                }
                                TextField {
                                    id: "random-count", label: "题目数量", value: random_count,
                                    touched: count_touched, disabled: fixed, numeric: true,
                                    error: bounded_integer(&random_count.read(), size).err(),
                                }
                            }
                            p { class: "field-label", "题库共 {size} 题 · 题号从 1 开始" }
                            if let Some(error) = launch_error() {
                                p { class: "field-error", role: "alert", "{error}" }
                            }
                            button { class: "primary-button panel-action", r#type: "button",
                                onclick: move |_| {
                                    let selection = if fixed {
                                        start_touched.set(true);
                                        end_touched.set(true);
                                        bounded_integer(&range_start.read(), size).and_then(|start| {
                                            bounded_integer(&range_end.read(), size).map(|end| Selection::Fixed { start, end })
                                        })
                                    } else {
                                        count_touched.set(true);
                                        bounded_integer(&random_count.read(), size).map(|count| Selection::Random { count })
                                    };
                                    match selection {
                                        Ok(selection) => match selection.indices(size) {
                                            Ok(_) => { launch_error.set(None); active.set(Some(selection)); }
                                            Err(error) => launch_error.set(Some(error)),
                                        },
                                        Err(error) => launch_error.set(Some(error.to_owned())),
                                    }
                                }, "开始"
                            }
                        }
                        }
                    }
                }
                div { class: "menu-entry",
                    button {
                        class: "menu-button", r#type: "button",
                        aria_expanded: expanded() == Some(Panel::Join),
                        aria_controls: "join-panel",
                        onclick: move |_| expanded.set(toggle_panel(expanded(), Panel::Join)),
                        "加入房间"
                    }
                    div {
                        class: "panel-collapse",
                        "data-open": expanded() == Some(Panel::Join),
                        aria_hidden: expanded() != Some(Panel::Join),
                        "inert": if expanded() != Some(Panel::Join) { Some("") } else { None },
                        div { class: "panel-collapse-inner",
                        section { class: "input-panel", id: "join-panel", aria_label: "加入房间设置",
                            TextField {
                                id: "player-name", label: "名字", value: player_name,
                                touched: player_name_touched,
                                error: player_name_error(&player_name.read()),
                            }
                            TextField {
                                id: "join-code", label: "房间号", value: join_code,
                                touched: join_touched, code: true, numeric: true,
                                error: room_code_error(&join_code.read()),
                            }
                            button { class: "primary-button panel-action", onclick: move |_| {
                                join_touched.set(true);
                                player_name_touched.set(true);
                                if let Some(message) = room_code_error(&join_code.read()).or_else(|| player_name_error(&player_name.read())) {
                                    network_error.set(Some(message.into())); return;
                                }
                                adverts.clear();
                                let result = discover_service.as_ref().map_err(Clone::clone).and_then(|service| service.command(Command::Discover { code: join_code() }));
                                network_error.set(result.err());
                            }, "搜索房间" }
                            p { class: "field-label", "搜索后请选择房间；相同房间号不会自动加入。" }
                            for advert in adverts() {
                                {
                                    let join_service = service.clone();
                                    rsx! {
                                        button { class: "secondary-button room-discovery", onclick: move |_| {
                                            if let Some(message) = player_name_error(&player_name.read()) { network_error.set(Some(message.into())); return; }
                                            network_error.set(join_service.as_ref().map_err(Clone::clone).and_then(|service| service.command(Command::Join { endpoint: advert.endpoint.clone(), room_id: Some(advert.room_id.clone()), player: player_name().trim().to_owned() })).err());
                                        }, "{advert.name} · {advert.code} · {advert.endpoint} · 任期 {advert.term}" }
                                    }
                                }
                            }
                            label { class: "field room-manual", "手动 IP:端口",
                                input { class: "text-input", value: manual_endpoint(), placeholder: "192.168.1.2:端口", oninput: move |event| manual_endpoint.set(event.value()) }
                            }
                            button { class: "secondary-button panel-action", onclick: move |_| {
                                if let Some(message) = player_name_error(&player_name.read()) { network_error.set(Some(message.into())); return; }
                                network_error.set(manual_service.as_ref().map_err(Clone::clone).and_then(|service| service.command(Command::Join { endpoint: manual_endpoint().trim().to_owned(), room_id: None, player: player_name().trim().to_owned() })).err());
                            }, "连接指定地址" }
                        }
                        }
                    }
                }
                div { class: "menu-entry",
                    button {
                        class: "menu-button", r#type: "button",
                        aria_expanded: expanded() == Some(Panel::Create),
                        aria_controls: "create-panel",
                        onclick: move |_| expanded.set(toggle_panel(expanded(), Panel::Create)),
                        "创建房间"
                    }
                    div {
                        class: "panel-collapse",
                        "data-open": expanded() == Some(Panel::Create),
                        aria_hidden: expanded() != Some(Panel::Create),
                        "inert": if expanded() != Some(Panel::Create) { Some("") } else { None },
                        div { class: "panel-collapse-inner",
                        section { class: "input-panel", id: "create-panel", aria_label: "创建房间设置",
                            TextField {
                                id: "create-code", label: "房间号", value: create_code,
                                touched: create_touched, code: true, numeric: true,
                                error: room_code_error(&create_code.read()),
                            }
                            TextField {
                                id: "room-name", label: "房间名称", value: room_name,
                                touched: name_touched,
                                error: room_name_error(&room_name.read()),
                            }
                            TextField { id: "creator-name", label: "名字", value: creator_name, touched: creator_touched, error: player_name_error(&creator_name.read()) }
                            button { class: "primary-button panel-action", r#type: "button", onclick: move |_| {
                                create_touched.set(true); name_touched.set(true); creator_touched.set(true);
                                if let Some(message) = room_code_error(&create_code.read()).or_else(|| room_name_error(&room_name.read())).or_else(|| player_name_error(&creator_name.read())) {
                                    network_error.set(Some(message.into())); return;
                                }
                                network_error.set(create_service.as_ref().map_err(Clone::clone).and_then(|service| service.command(Command::Create { name: room_name().trim().to_owned(), code: create_code(), player: creator_name().trim().to_owned() })).err());
                            }, "创建" }
                        }
                        }
                    }
                }
            }
            }
            }
            footer { class: "app-footer",
                a {
                    href: "https://github.com/QT-hook/300-Questions",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "GitHub · QT-hook/300-Questions"
                }
            }
        }
    }
}

fn bounded_integer(value: &str, size: usize) -> Result<usize, &'static str> {
    let value = positive_integer(value)?;
    let value = usize::try_from(value).map_err(|_| "数值过大")?;
    if value > size {
        return Err("不能超过题库总题数");
    }
    Ok(value)
}
