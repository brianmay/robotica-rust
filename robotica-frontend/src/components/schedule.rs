//! Component that shows the schedule

use chrono::Local;
use itertools::Itertools;
use serde_json::Value;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use robotica_common::{
    datetime::datetime_to_string,
    mqtt::{Json, MqttMessage},
    robotica::tasks::Task,
    scheduler::{Mark, MarkStatus, Sequence},
};

use crate::services::websocket::WebsocketService;

/// The yew properties for the websocket client
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

/// Component that shows the schedule
#[function_component(Schedule)]
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

    let expanded_id = use_state(|| None);

    let expanded_id_clone: UseStateHandle<Option<String>> = expanded_id.clone();
    let on_click = Callback::from(move |id: String| {
        expanded_id_clone.set(Some(id));
    });

    let expanded_id_clone: UseStateHandle<Option<String>> = expanded_id.clone();
    let on_close = Callback::from(move |_| {
        expanded_id_clone.set(None);
    });

    use_mut_ref(move || {
        let topic = props.topic.clone();
        let mut wss = wss;
        spawn_local(async move {
            let sub = wss.subscribe_mqtt(topic, callback).await;
            *subscription.borrow_mut() = Some(sub);
        });
    });

    let modal_open_class = match *expanded_id {
        Some(_) => "modal-open",
        None => "",
    };
    let classes = classes!("sequence_list", modal_open_class);

    let expanded_id = &*expanded_id;
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
    sequence.required_time.with_timezone(&Local).date_naive()
}

fn sequence_list_to_html<'a>(
    sequence_list: impl Iterator<Item = &'a Sequence>,
    expanded_id: &Option<String>,
    on_click: &Callback<String>,
    on_close: &Callback<()>,
) -> Html {
    html! {
        <div class="sequence_list">
        {
            sequence_list.map(|sequence| {
                sequence_to_html(sequence, expanded_id, on_click, on_close)
            }).collect::<Html>()
        }
        </div>
    }
}

fn sequence_to_html(
    sequence: &Sequence,
    expanded_id: &Option<String>,
    on_click: &Callback<String>,
    on_close: &Callback<()>,
) -> Html {
    let importance_class = match sequence.importance {
        robotica_common::scheduler::Importance::NotImportant => "not_important",
        robotica_common::scheduler::Importance::Important => "important",
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

    let local = sequence.required_time.with_timezone(&Local);
    let time = local.format("%H:%M:%S").to_string();


    let classes = classes!("sequence", importance_class, mark_class);
    html! {
        <div class={classes} id={sequence.id.clone()}>
            <div>{time}</div>
            <div>
                <div class="title">{&sequence.title}</div>
                {
                    sequence.tasks.iter().enumerate().map(|(i, task)| {
                        task_to_html(sequence, task, i, expanded_id, on_click, on_close)
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
    expanded_id: &Option<String>,
    on_click: &Callback<String>,
    on_close: &Callback<()>,
) -> Html {
    let date = sequence.required_time.date_naive();
    let seq_id = &sequence.id;
    let repeat_number = sequence.repeat_number;
    let id = format!("{date}-{seq_id}-{i}-{repeat_number}");
    let id_clone = id.clone();
    let on_click = on_click.clone();

    html! {
        html! {
            <>
                <div class="task" onclick={move |_| on_click.emit(id.clone())}><span>{&task.title}</span></div>
                {
                    if Some(id_clone) == *expanded_id {
                        popover_content(sequence, task, on_close)
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
        Value::Number(n) => html! { <span class="number">{n}</span> },
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

fn popover_content(sequence: &Sequence, task: &Task, on_close: &Callback<()>) -> Html {
    use robotica_common::robotica::tasks::Payload;
    let payload = match &task.payload {
        Payload::String(string) => html!({ string.clone() }),
        Payload::Json(json) => json_to_html(json),
    };
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
                            <h1 class="modal-title fs-5" id="exampleModalLabel">{"Details"}</h1>
                            <button type="button" class="btn-close" aria-label="Close" onclick={on_close.clone()}></button>
                        </div>
                        <div class="modal-body">
                        <table class="table">
                        <tbody>
                            <tr>
                                <th scope="row">{"Required Time"}</th>
                                <td>{datetime_to_string(&sequence.required_time)}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Required Duration"}</th>
                                <td>{sequence.required_duration}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Latest Time"}</th>
                                <td>{datetime_to_string(&sequence.latest_time)}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Repeat Number"}</th>
                                <td>{sequence.repeat_number}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Importance"}</th>
                                <td>{sequence.importance}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Mark"}</th>
                                <td>{mark}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Topics"}</th>
                                <td>{task.topics.clone()}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Title"}</th>
                                <td>{task.title.clone()}</td>
                            </tr>
                            <tr>
                                <th scope="row">{"Summary"}</th>
                                <td>{task}</td>
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
                                <td>{task.retain}</td>
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
