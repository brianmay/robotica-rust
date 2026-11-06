//! Component that shows and manages reminders.

use chrono::Local;
use robotica_common::{
    config::{reminder_command_topic, reminders_topic},
    datetime::datetime_to_string,
    mqtt::{Json, MqttMessage, QoS, Retain},
    robotica::reminder::{ReminderCommand, ReminderInfo, ReminderStatus},
};
use tracing::error;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::services::websocket::WebsocketService;

/// Component that shows the current reminders.
#[function_component(ReminderView)]
pub fn reminder_view() -> Html {
    let wss = use_context::<WebsocketService>().unwrap();
    let reminders = use_state(Vec::<ReminderInfo>::new);
    let subscription = use_mut_ref(|| None);

    let callback = {
        let reminders = reminders.clone();
        Callback::from(move |msg: MqttMessage| {
            msg.try_into().map_or_else(
                |e| {
                    error!("Failed to parse reminder list: {e}");
                },
                |Json(list): Json<Vec<ReminderInfo>>| reminders.set(list),
            );
        })
    };

    {
        let mut wss = wss.clone();
        use_mut_ref(move || {
            spawn_local(async move {
                let sub = wss.subscribe_mqtt(reminders_topic(), callback).await;
                *subscription.borrow_mut() = Some(sub);
            });
        });
    }

    let send = {
        Callback::from(move |command: ReminderCommand| {
            let mut wss = wss.clone();
            match MqttMessage::from_json(
                reminder_command_topic(),
                &command,
                Retain::NoRetain,
                QoS::ExactlyOnce,
            ) {
                Ok(msg) => wss.send_mqtt(msg),
                Err(err) => error!("Failed to serialize reminder command: {err}"),
            }
        })
    };

    html! {
        <div class="reminder_list">
            <h1>{ "Reminders" }</h1>
            {
                if reminders.is_empty() {
                    html! { <div class="reminder_empty">{ "No reminders" }</div> }
                } else {
                    reminders
                        .iter()
                        .map(|reminder| reminder_to_html(reminder, &send))
                        .collect::<Html>()
                }
            }
        </div>
    }
}

fn reminder_to_html(reminder: &ReminderInfo, send: &Callback<ReminderCommand>) -> Html {
    let status_class = match reminder.status {
        ReminderStatus::Active => "reminder_active",
        ReminderStatus::Acknowledged => "reminder_acknowledged",
        ReminderStatus::Silenced => "reminder_silenced",
    };

    let is_active = reminder.status == ReminderStatus::Active;
    let is_silenced = reminder.status == ReminderStatus::Silenced;

    let created = reminder
        .created_at
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let silenced_until = reminder
        .silenced_until
        .map_or_else(|| "indefinitely".to_string(), datetime_to_string);

    let acknowledge = {
        let send = send.clone();
        let id = reminder.id.clone();
        Callback::from(move |_| {
            send.emit(ReminderCommand::Acknowledge { id: id.clone() });
        })
    };

    let unsilence = {
        let send = send.clone();
        let key = reminder.key.clone();
        Callback::from(move |_| {
            send.emit(ReminderCommand::Unsilence { key: key.clone() });
        })
    };

    html! {
        <div class={classes!("reminder", status_class)}>
            <div class="reminder_header">
                <span class="reminder_title">{ &reminder.message.title }</span>
                <span class="reminder_status">{ format!("{}", reminder.status) }</span>
            </div>
            <div class="reminder_body">{ &reminder.message.body }</div>
            <div class="reminder_meta">
                <span>{ format!("Priority: {}", reminder.message.priority) }</span>
                <span>{ format!("Audience: {}", reminder.message.audience) }</span>
                <span>{ format!("Created: {created}") }</span>
                <span>{ format!("Key: {}", reminder.key) }</span>
                if is_silenced {
                    <span>{ format!("Silenced: {silenced_until}") }</span>
                }
            </div>
            <div class="reminder_actions">
                if is_active {
                    <button class="btn btn-sm btn-primary" onclick={acknowledge}>
                        { "Acknowledge" }
                    </button>
                }
                if is_silenced {
                    <button class="btn btn-sm btn-secondary" onclick={unsilence}>
                        { "Unsilence" }
                    </button>
                } else {
                    { silence_buttons(&reminder.key, send) }
                }
            </div>
        </div>
    }
}

fn silence_buttons(key: &str, send: &Callback<ReminderCommand>) -> Html {
    let buttons = [
        ("1h", Some(60)),
        ("8h", Some(480)),
        ("24h", Some(1440)),
        ("Forever", None),
    ];

    buttons
        .into_iter()
        .map(|(label, duration_minutes)| {
            let send = send.clone();
            let key = key.to_string();
            let onclick = Callback::from(move |_| {
                send.emit(ReminderCommand::Silence {
                    key: key.clone(),
                    duration_minutes,
                });
            });
            html! {
                <button class="btn btn-sm btn-outline-secondary" {onclick}>
                    { format!("Silence {label}") }
                </button>
            }
        })
        .collect::<Html>()
}
