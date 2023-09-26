//! Component that shows the schedule

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use robotica_common::{
    datetime::datetime_to_string,
    mqtt::{Json, MqttMessage},
    robotica::tasks::Task,
    scheduler::Sequence,
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

    use_mut_ref(move || {
        let topic = props.topic.clone();
        let mut wss = wss;
        spawn_local(async move {
            let sub = wss.subscribe_mqtt(topic, callback).await;
            *subscription.borrow_mut() = Some(sub);
        });
    });

    let expanded_id = &*expanded_id;
    html! {
        <div class="sequence_list">
        {
            sequence_list.iter().map(move |sequence| {
                sequence_to_html(sequence, expanded_id, &on_click)
            }).collect::<Html>()
        }
        </div>
    }
}

fn sequence_to_html(
    sequence: &Sequence,
    expanded_id: &Option<String>,
    on_click: &Callback<String>,
) -> Html {
    html! {
        <div class="sequence" id={sequence.id.clone()}>
            <div>{datetime_to_string(&sequence.required_time)}</div>
            <div>
            {
                sequence.tasks.iter().enumerate().map(|(i, task)| {
                    task_to_html(sequence, task, i, expanded_id, on_click.clone())
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
    on_click: Callback<String>,
) -> Html {
    let date = sequence.required_time.date_naive();
    let seq_id = &sequence.id;
    let repeat_number = sequence.repeat_number;
    let id = format!("{date}-{seq_id}-{i}-{repeat_number}");
    let id_clone = id.clone();

    html! {
        html! {
            <>
                <div onclick={move |_| on_click.emit(id.clone())}>{&task.title}</div>
                {
                    if Some(id_clone) == *expanded_id {
                        popover_content(sequence, task)
                    } else { html! {} }
                }
            </>
        }
    }
}

fn popover_content(sequence: &Sequence, task: &Task) -> Html {
    use robotica_common::robotica::tasks::Payload;
    let description = task
        .title
        .clone();
    let payload = match &task.payload {
        Payload::String(string) => string.clone(),
        Payload::Json(json) => json.to_string(),
    };
    let mark = match sequence.mark.clone() {
        Some(mark) => format!("{mark:?}"),
        None => "None".to_string(),
    };

    html! {
        <div role="tooltip">
            <h3 class="popover-title">{description}</h3>
            <div class="popover-content">
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
                            <th scope="row">{"Mark"}</th>
                            <td>{mark}</td>
                        </tr>
                        <tr>
                            <th scope="row">{"Topics"}</th>
                            <td>{task.topics.clone()}</td>
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
        </div>
    }
}
