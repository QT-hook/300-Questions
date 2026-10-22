use std::{rc::Rc, time::Duration};

use dioxus::prelude::*;

use crate::{
    practice::{Phase, Selection, Session},
    questions::Question,
};

#[component]
pub fn PracticeScreen(
    bank: Rc<Vec<Question>>,
    selection: Selection,
    onhome: EventHandler,
) -> Element {
    let initial = Session::new(selection, bank.len(), 1);
    let mut state = use_signal(move || initial);
    let mut timer = use_signal(|| None::<dioxus::core::Task>);
    let snapshot = state.read().clone();
    let session = match snapshot {
        Ok(session) => session,
        Err(error) => {
            return rsx! {
                section { class: "practice-shell", role: "alert",
                    p { "无法开始练习：{error}" }
                    button { class: "primary-button", onclick: move |_| onhome.call(()), "返回主页" }
                }
            };
        }
    };
    let generation = session.generation;
    if session.phase == Phase::Finished {
        let restart_bank = bank.clone();
        return rsx! {
            section { class: "practice-shell result", aria_label: "练习结果",
                h1 { "练习完成" }
                p { "总题数：{session.indices.len()}" }
                p { "答对题数：{session.correct}" }
                p { "正确率：{session.accuracy()}%" }
                div { class: "practice-actions",
                    button { class: "secondary-button", onclick: move |_| {
                        if let Some(task) = timer.take() { task.cancel(); }
                        onhome.call(());
                    }, "返回主页" }
                    button { class: "primary-button", onclick: move |_| {
                        if let Some(task) = timer.take() { task.cancel(); }
                        state.set(Session::new(selection, restart_bank.len(), generation + 1));
                    }, "再练一次" }
                }
            }
        };
    }
    let position = session.position;
    let question = &bank[session.indices[position]];
    let correct = question.answer_index();
    let phase_class = match session.phase {
        Phase::Leaving => "question-card leaving",
        Phase::Entering => "question-card entering",
        _ => "question-card",
    };
    let feedback = session.selected.map(|selected| {
        if selected == correct {
            "回答正确".to_owned()
        } else {
            format!(
                "回答错误。正确答案：{} · {}",
                question.answer, question.options[correct]
            )
        }
    });
    rsx! {
        section { class: "practice-shell", aria_label: "答题练习",
            header { class: "practice-header",
                button { class: "secondary-button", onclick: move |_| {
                    if let Some(task) = timer.take() { task.cancel(); }
                    onhome.call(());
                }, "返回主页" }
                span { "进度 {position + 1} / {session.indices.len()} · 原题 {session.indices[position] + 1}" }
            }
            div { class: "question-viewport",
                article { class: phase_class,
                    h1 { class: "question-title", "{question.question}" }
                    div { class: "answer-options", role: "group", aria_label: "选择答案",
                        for (index, option) in question.options.iter().enumerate() {
                            button {
                                class: if session.selected.is_some() && index == correct { "answer-option correct" }
                                    else if session.selected == Some(index) { "answer-option wrong" }
                                    else { "answer-option" },
                                r#type: "button", disabled: session.phase != Phase::Answering,
                                onclick: move |_| {
                                    let accepted = {
                                        let mut guard = state.write();
                                        guard.as_mut().is_ok_and(|session| {
                                            session.generation == generation && session.position == position
                                                && session.answer(index, correct)
                                        })
                                    };
                                    if !accepted { return; }
                                    let task = spawn(async move {
                                        tokio::time::sleep(Duration::from_millis(if index == correct { 400 } else { 2000 })).await;
                                        if !advance(state, generation, position, Phase::Feedback) { return; }
                                        tokio::time::sleep(Duration::from_millis(300)).await;
                                        if !advance(state, generation, position, Phase::Leaving) { return; }
                                        tokio::time::sleep(Duration::from_millis(300)).await;
                                        advance(state, generation, position + 1, Phase::Entering);
                                    });
                                    timer.set(Some(task));
                                },
                                span { class: "option-letter", "{['A', 'B', 'C', 'D'][index]}" }
                                span { "{option}" }
                            }
                        }
                    }
                    div { class: "answer-feedback", role: "status", aria_live: "polite",
                        if let Some(message) = feedback { "{message}" }
                    }
                }
            }
        }
    }
}

fn advance(
    mut state: Signal<Result<Session, String>>,
    generation: u64,
    position: usize,
    phase: Phase,
) -> bool {
    state
        .write()
        .as_mut()
        .is_ok_and(|session| session.advance(generation, position, phase))
}
