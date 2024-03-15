//! Component that shows the schedule

use chrono::Local;
use itertools::Itertools;
use serde_json::Value;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use robotica_common::{
    datetime::{datetime_to_string, DurationExt},
    mqtt::{Json, MqttMessage},
    robotica::tasks::Task,
    scheduler::{Mark, MarkStatus, Sequence},
};

use crate::services::websocket::WebsocketService;

/// The yew properties for the schedule component
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

#[derive(Default, Eq, PartialEq)]
enum OpenedId {
    Sequence(String),
    Task(String),
    #[default]
    None,
}

/// Component that shows the schedule
#[function_component(RoboticaSchedule)]
pub fn schedule(props: &Props) -> Html {
    let wss: WebsocketService = use_context().unwrap();
    let subscription = use_mut_ref(|| None);
    let sequence_list = use_state(std::vec::Vec::new);

    let callback = {
        let sequence_list = sequence_list.clone();
        Callback::from(move |msg: MqttMessage| {
            msg.try_into().map_or_else(
                |e| {
                    tracing::error!("Failed to parse schedule: {}", e);
                },
                |Json(new_schedule): Json<Vec<Sequence>>| sequence_list.set(new_schedule),
            );
        })
    };

    let opened_id = use_state(OpenedId::default);

    let opened_id_clone: UseStateHandle<OpenedId> = opened_id.clone();
    let on_click = Callback::from(move |id: OpenedId| {
        opened_id_clone.set(id);
    });

    let opened_id_clone: UseStateHandle<OpenedId> = opened_id.clone();
    let on_close = Callback::from(move |()| {
        opened_id_clone.set(OpenedId::None);
    });

    use_mut_ref(move || {
        let topic = props.topic.clone();
        let mut wss = wss;
        spawn_local(async move {
            let sub = wss.subscribe_mqtt(topic, callback).await;
            *subscription.borrow_mut() = Some(sub);
        });
    });

    let modal_open_class = if matches!(*opened_id, OpenedId::None) {
        ""
    } else {
        "modal-open"
    };
    let classes = classes!("sequence_list", modal_open_class);

    let expanded_id = &*opened_id;
    html! {
        <div class={classes}>
        {
            sequence_list.iter().group_by(|s| get_local_date_for_sequence(s)).into_iter().map(|(date, sequence_list)| {
                let date_string = date.format("%A, %e %B, %Y").to_string();
                html! {
                    <>
                        <h2>{date_string}</h2>
                        {
                            sequence_list_to_html(sequence_list, expanded_id, &on_click, &on_close)
                        }
                    </>
                }
            }).collect::<Html>()

        }
        </div>
    }
}

fn get_local_date_for_sequence(sequence: &Sequence) -> chrono::NaiveDate {
    sequence.start_time.with_timezone(&Local).date_naive()
}

fn sequence_list_to_html<'a>(
    sequence_list: impl Iterator<Item = &'a Sequence>,
    opened_id: &OpenedId,
    on_click: &Callback<OpenedId>,
    on_close: &Callback<()>,
) -> Html {
    html! {
        <div class="sequence_list">
        {
            sequence_list.map(|sequence| {
                sequence_to_html(sequence, opened_id, on_click, on_close)
            }).collect::<Html>()
        }
        </div>
    }
}

fn sequence_to_html(
    sequence: &Sequence,
    opened_id: &OpenedId,
    on_click: &Callback<OpenedId>,
    on_close: &Callback<()>,
) -> Html {
    let importance_class = match sequence.importance {
        robotica_common::scheduler::Importance::Low => "importance_low",
        robotica_common::scheduler::Importance::Medium => "importance_medium",
        robotica_common::scheduler::Importance::High => "importance_high",
    };
    let status_class = match sequence.status {
        Some(robotica_common::scheduler::Status::Pending) | None => "pending",
        Some(robotica_common::scheduler::Status::InProgress) => "in_progress",
        Some(robotica_common::scheduler::Status::Completed) => "completed",
        Some(robotica_common::scheduler::Status::Cancelled) => "cancelled",
    };
    let mark_class = match sequence.mark {
        Some(Mark {
            status: MarkStatus::Cancelled,
            ..
        }) => Some("cancelled"),
        Some(Mark {
            status: MarkStatus::Done,
            ..
        }) => Some("done"),
        None => None,
    };

    let start_local = sequence.start_time.with_timezone(&Local);
    let end_local = sequence.end_time.with_timezone(&Local);
    let days = (end_local.date_naive() - start_local.date_naive()).num_days();

    let start_str = start_local.format("%H:%M:%S").to_string();
    let end_str = end_local.format("%H:%M:%S").to_string();

    let end_str = if days > 0 {
        format!("{end_str}+{days}")
    } else {
        end_str
    };

    let date = sequence.schedule_date;
    let seq_id = &sequence.id;
    let repeat_number = sequence.repeat_number;
    let id = format!("{date}-{seq_id}-{repeat_number}");
    let id_clone = id.clone();

    let on_click_clone = on_click.clone();

    let classes = classes!("sequence", importance_class, status_class, mark_class);
    html! {
        <div class={classes} id={sequence.id.clone()}>
            <div>{start_str}{" - "}{end_str}</div>
            <div>
                <div class="title" onclick={move |_| on_click_clone.emit(OpenedId::Sequence(id_clone.clone()))}><span>{&sequence.title}</span></div>
                {
                    if OpenedId::Sequence(id.clone()) == *opened_id {
                        popover_sequence_content(sequence, on_close)
                    } else { html! {} }
                }
                {
                    sequence.tasks.iter().enumerate().map(|(i, task)| {
                        task_to_html(sequence, task, i, opened_id, on_click, on_close)
                    }).collect::<Html>()
                }
            </div>
        </div>
    }
}

fn task_to_html(
    sequence: &Sequence,
    task: &Task,
    i: usize,
    opened_id: &OpenedId,
    on_click: &Callback<OpenedId>,
    on_close: &Callback<()>,
) -> Html {
    let date = sequence.schedule_date;
    let seq_id = &sequence.id;
    let repeat_number = sequence.repeat_number;
    let id = format!("{date}-{seq_id}-{repeat_number}-{i}");
    let id_clone = id.clone();
    let on_click = on_click.clone();

    html! {
        html! {
            <>
                <div class="task" onclick={move |_| on_click.emit(OpenedId::Task(id_clone.clone()))}><span>{&task.title}</span></div>
                {
                    if OpenedId::Task(id) == *opened_id {
                        popover_task_content(task, on_close)
                    } else { html! {} }
                }
            </>
        }
    }
}

fn json_to_html(json: &Value) -> Html {
    match json {
        Value::Null => html! { <span class="null">{"null"}</span> },
        Value::Bool(b) => html! { <span class="bool">{b}</span> },
        Value::Number(n) => html! { <span class="number">{n.to_string()}</span> },
        Value::String(s) => html! { <span class="string">{s}</span> },
        Value::Array(a) => {
            html! {
                <ul class="array">
                    {
                        a.iter().map(|v| {
                            html! {
                                <li>{json_to_html(v)}</li>
                            }
                        }).collect::<Html>()
                    }
                </ul>
            }
        }
        Value::Object(o) => {
            html! {
                <table class="table">
                    <tbody>
                    {
                        o.iter().map(|(k, v)| {
                            html! {
                                <tr>
                                    <th scope="row">{k}</th>
                                    <td>{json_to_html(v)}</td>
                                </tr>
                            }
                        }).collect::<Html>()
                    }
                    </tbody>
                </table>
            }
        }
    }
}

fn popover_sequence_content(sequence: &Sequence, on_close: &Callback<()>) -> Html {
    let mark = match sequence.mark.clone() {
        Some(mark) => format!("{mark:?}"),
        None => "None".to_string(),
    };

    let on_close = on_close.clone();
    let on_close = move |_| {
        on_close.emit(());
    };

    html! {
        <>
            <div class="modal fade show" tabindex="-1" style="display:block" onclick={on_close.clone()}>
                <div class="modal-dialog">
                    <div class="modal-content">
                        <div class="modal-header">
                            <h1 class="modal-title fs-5">{"Sequence: "}{&sequence.title}</h1>
                            <button type="button" class="btn-close" aria-label="Close" onclick={on_close.clone()}></button>
                        </div>
                        <div class="modal-body">
                        <table class="table">
                        <tbody>
                            <tr>
                                <th scope="row">{"Id"}</th>
                                <td>{&sequence.id}</td>
                            </tr>
                            { if let Some(status) = &sequence.status {
                                html! {
                                    <tr>
                                        <th scope="row">{"Status"}</th>
                                        <td>{status.to_string()}</td>
                                    </tr>
                                }
                            } else { html! {} } }
                            { if let Some(mark) = &sequence.mark {
                                html! {
                                    <tr>
                                        <th scope="row">{"Marks"}</th>
                                        <td>{mark.to_string()}</td>
                                    </tr>
                                }
                            } else { html! {} } }
                            <tr>
                                <th scope="row">{"Required Time"}</th>
                                <td>{datetime_to_string(&sequence.start_time)}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Required Duration"}</th>
                                <td>{sequence.duration.to_string()}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Latest Time"}</th>
                                <td>{datetime_to_string(&sequence.latest_time)}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Schedule Date"}</th>
                                <td>{sequence.schedule_date.to_string()}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Repeat Number"}</th>
                                <td>{sequence.repeat_number}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Importance"}</th>
                                <td>{sequence.importance.to_string()}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Mark"}</th>
                                <td>{mark}</td>
                            </tr>
                        </tbody>
                    </table>
                        </div>
                        <div class="modal-footer">
                            <button type="button" class="btn btn-secondary" onclick={on_close}>{"Close"}</button>
                        </div>
                    </div>
                </div>
            </div>
            <div class="modal-backdrop fade show"></div>
        </>
    }
}

fn popover_task_content(task: &Task, on_close: &Callback<()>) -> Html {
    use robotica_common::robotica::tasks::Payload;

    let payload = match &task.payload {
        Payload::String(string) => html!({ string.clone() }),
        Payload::Json(json) => json_to_html(json),
        Payload::Command(command) => html!({
            match serde_json::to_value(command) {
                Ok(value) => json_to_html(&value),
                Err(err) => html! { <span class="error">{err.to_string()}</span> },
            }
        }),
    };

    let on_close = on_close.clone();
    let on_close = move |_| {
        on_close.emit(());
    };

    html! {
        <>
            <div class="modal fade show" tabindex="-1" style="display:block" onclick={on_close.clone()}>
                <div class="modal-dialog">
                    <div class="modal-content">
                        <div class="modal-header">
                            <h1 class="modal-title fs-5">{"Task: "}{&task.title}</h1>
                            <button type="button" class="btn-close" aria-label="Close" onclick={on_close.clone()}></button>
                        </div>
                        <div class="modal-body">
                        <table class="table">
                        <tbody>
                            <tr>
                                <th scope="row">{"Topics"}</th>
                                <td>{task.topics.clone()}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Summary"}</th>
                                <td>{task.to_string()}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Payload"}</th>
                                <td>{payload}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"QoS"}</th>
                                <td>{format!("{:?}", task.qos)}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Retain"}</th>
                                <td>{format!("{:?}", task.retain)}</td>
                            </tr>
                        </tbody>
                    </table>
                        </div>
                        <div class="modal-footer">
                            <button type="button" class="btn btn-secondary" onclick={on_close}>{"Close"}</button>
                        </div>
                    </div>
                </div>
            </div>
            <div class="modal-backdrop fade show"></div>
        </>
    }
}
