use std::{rc::Rc, time::Duration};

use dioxus::prelude::*;

use super::{
    model::{Mode, Room},
    service::{Command, Service},
};
use crate::{practice::Selection, questions::Question};

#[derive(Clone, Props)]
pub struct RoomScreenProps {
    pub service: Rc<Service>,
    pub room: Room,
    pub bank: Rc<Vec<Question>>,
    pub onleave: EventHandler,
}
impl PartialEq for RoomScreenProps {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.service, &other.service)
            && self.room == other.room
            && Rc::ptr_eq(&self.bank, &other.bank)
            && self.onleave == other.onleave
    }
}

#[component]
pub fn RoomScreen(props: RoomScreenProps) -> Element {
    let RoomScreenProps {
        service,
        room,
        bank,
        onleave,
    } = props;
    let mut error = use_signal(|| None::<String>);
    let mut random = use_signal(|| false);
    let mut start = use_signal(|| "1".to_owned());
    let mut end = use_signal(|| "10".to_owned());
    let mut count = use_signal(|| "10".to_owned());
    let mut mode = use_signal(|| Mode::Independent);
    let mut local_position = use_signal(|| 0usize);
    let mut selected = use_signal(|| None::<(usize, usize)>);
    let mut phase = use_signal(|| 0u8);
    let host = room.host == service.id();
    let leave_service = service.clone();
    let configure_service = service.clone();
    let ready_service = service.clone();
    let start_service = service.clone();
    let size = bank.len();
    let started = room.started;
    let mut config_valid = use_signal(|| false);
    use_effect(use_reactive!(|(host, started)| {
        let random = random();
        let start = start();
        let end = end();
        let count = count();
        let mode = mode();
        if !host || started {
            return;
        }
        let parse = |value: &str| {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("请输入有效的正整数".to_owned());
            }
            value
                .parse::<usize>()
                .map_err(|_| "请输入有效的正整数".to_owned())
        };
        let selection = if random {
            parse(&count).map(|count| Selection::Random { count })
        } else {
            parse(&start).and_then(|start| parse(&end).map(|end| Selection::Fixed { start, end }))
        };
        let result = selection
            .and_then(|selection| selection.indices(size))
            .and_then(|indices| configure_service.command(Command::Configure { mode, indices }));
        config_valid.set(result.is_ok());
        error.set(result.err());
    }));
    let correct_answers: Vec<_> = bank.iter().map(Question::answer_index).collect();
    let position = if room.mode == Mode::Sync {
        room.cursor
    } else {
        local_position()
    };
    let current = room
        .indices
        .get(position)
        .and_then(|&index| bank.get(index));
    let own_answer = room
        .answers
        .iter()
        .find(|answer| answer.member == service.id() && answer.position == position)
        .map(|answer| answer.choice);
    let choice = selected()
        .filter(|(p, _)| *p == position)
        .map(|(_, c)| c)
        .or(own_answer);
    let reveal = room.mode == Mode::Independent && choice.is_some()
        || room.mode == Mode::Sync && room.reveal_until.is_some();
    rsx! {
        section { class: "practice-shell room-shell",
            header { class: "practice-header",
                button { class: "secondary-button", onclick: move |_| {
                    match leave_service.command(Command::Leave) {
                        Ok(()) => onleave.call(()), Err(message) => error.set(Some(message)),
                    }
                }, "离开房间" }
                span { "{room.name} · 房间号 {room.code} · 任期 {room.term}" }
            }
            p { class: "room-endpoint", "本机地址：{service.endpoint()}" }
            if let Some(notice) = &room.notice { p { role: "status", "{notice}" } }
            if let Some(message) = error() { p { class: "field-error", role: "alert", "{message}" } }
            if !room.started {
                section { class: "question-card room-config",
                    h1 { class: "question-title", "等待开始" }
                    p { "已配置 {room.indices.len()} 题 · " if room.mode == Mode::Sync { "同步答题" } else { "独立答题" } }
                    if host {
                        label { class: "mode-option", input { r#type: "checkbox", checked: random(), onchange: move |event| random.set(event.checked()) } "随机题目" }
                        if random() {
                            label { class: "field", "题目数量" input { class: "text-input", value: count(), oninput: move |event| count.set(event.value()) } }
                        } else {
                            div { class: "range-inputs",
                                label { class: "field", "起始题号" input { class: "text-input", value: start(), oninput: move |event| start.set(event.value()) } }
                                span { "～" }
                                label { class: "field", "结束题号" input { class: "text-input", value: end(), oninput: move |event| end.set(event.value()) } }
                            }
                        }
                        div { class: "practice-actions",
                            label { input { r#type: "radio", name: "room-mode", checked: mode() == Mode::Sync, onchange: move |_| mode.set(Mode::Sync) } "同步" }
                            label { input { r#type: "radio", name: "room-mode", checked: mode() == Mode::Independent, onchange: move |_| mode.set(Mode::Independent) } "独立" }
                        }
                    } else { p { "由房主配置题目；准备后表示可以开始。" } }
                    div { class: "practice-actions",
                        button { class: "primary-button", disabled: room.members.iter().any(|member|member.id==service.id() && member.ready), onclick: move |_| error.set(ready_service.command(Command::Ready).err()), "准备" }
                        if host { button { class: "primary-button", disabled: !config_valid(), onclick: move |_| error.set(start_service.command(Command::Start).err()), "开始答题" } }
                    }
                }
            } else if let Some(question) = current {
                p { class: "field-label", "进度 {position + 1} / {room.indices.len()}" }
                div { class: "question-viewport",
                    article { key: "{position}", class: if phase() == 1 { "question-card leaving" } else if phase() == 2 { "question-card entering" } else if room.mode == Mode::Sync { "question-card sync-entering" } else { "question-card" },
                        h1 { class: "question-title", "{question.question}" }
                        div { class: "answer-options",
                            for (index, option) in question.options.iter().enumerate() {
                                button {
                                    class: if reveal && index == question.answer_index() { "answer-option correct" } else if reveal && choice == Some(index) { "answer-option wrong" } else { "answer-option" },
                                    disabled: choice.is_some() || phase() != 0 || reveal,
                                    onclick: {
                                        let service = service.clone();
                                        let independent = room.mode == Mode::Independent;
                                        let correct = question.answer_index();
                                        move |_| {
                                            if selected().is_some_and(|(p, _)| p == position) { return; }
                                            if let Err(message) = service.command(Command::Answer { position, choice: index }) { error.set(Some(message)); return; }
                                            selected.set(Some((position, index)));
                                            if independent {
                                                spawn(async move {
                                                    tokio::time::sleep(Duration::from_millis(if index == correct { 400 } else { 2000 })).await;
                                                    phase.set(1);
                                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                                    local_position.set(position + 1);
                                                    selected.set(None);
                                                    phase.set(2);
                                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                                    phase.set(0);
                                                });
                                            }
                                        }
                                    },
                                    span { class: "option-letter", "{['A','B','C','D'][index]}" }
                                    span { "{option}" }
                                }
                            }
                        }
                        div { class: "answer-feedback", role: "status", aria_live: "polite",
                            if reveal {
                                if choice == Some(question.answer_index()) { "回答正确" } else { "正确答案：{question.answer} · {question.options[question.answer_index()]}" }
                            } else if choice.is_some() { "答案已提交，等待其他玩家。" }
                        }
                    }
                }
            } else { section { class: "question-card", h1 { "答题完成" } p { "成绩实时更新，可等待其他玩家或离开房间。" } } }
            section { class: "room-members", aria_label: "成员与实时成绩",
                h2 { "成员与成绩" }
                for member in &room.members {
                    div { class: "room-member", key: "{member.id}",
                        span { "{member.name}" if member.id == room.host { " · 房主" } }
                        span {
                            if !member.online {
                                if room.started && room.completed(&member.id) { "已完成 · 已离开" }
                                else { "未完成 · 已离开" }
                            }
                            else if room.started && room.completed(&member.id) { "已完成" }
                            else if room.started { "答题中" }
                            else if member.ready { "已准备" } else { "未准备" }
                            " · 答对 {room.score(&member.id, &correct_answers)} / 总题数 {room.indices.len()}"
                            if !room.indices.is_empty() {
                                " · 正确率 "
                                {format!("{:.1}%", room.score(&member.id, &correct_answers) as f64 / room.indices.len() as f64 * 100.0)}
                            }
                        }
                    }
                }
            }
        }
    }
}
