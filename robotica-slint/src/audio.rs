//! Audio player service

use std::{collections::HashMap, path::Path, sync::Arc};

use robotica_backend::{
    entities::Receiver,
    services::{
        mqtt::{MqttTx, Subscriptions},
        persistent_state::PersistentStateDatabase,
    },
    spawn,
    tasks::get_task_messages,
};
use robotica_common::{
    mqtt::{Json, MqttMessage, QoS},
    robotica::{
        audio::State,
        commands::{AudioCommand, Command},
        tasks::Task,
    },
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::{
    command::{Error, ErrorKind, Line},
    ui::ScreenCommand,
};

#[derive(Deserialize)]
pub struct Config {
    topic_substr: String,
    targets: HashMap<String, String>,
}

pub fn run(
    tx_screen_command: mpsc::Sender<ScreenCommand>,
    subscriptions: &mut Subscriptions,
    mqtt: MqttTx,
    database: &PersistentStateDatabase,
    config: Arc<Config>,
) {
    let topic_substr = &config.topic_substr;
    let topic = format!("command/{topic_substr}");
    let command_rx: Receiver<Json<Command>> = subscriptions.subscribe_into_stateless(topic);
    let psr = database.for_name::<State>(topic_substr);
    let mut state = psr.load().unwrap_or_default();

    spawn(async move {
        let topic_substr = &config.topic_substr;

        let mut command_s = command_rx.subscribe().await;
        init_all(&state).await.unwrap_or_else(|err| {
            state.error = Some(err);
            state.play_list = None;
        });

        while let Ok(Json(command)) = command_s.recv().await {
            if let Command::Audio(command) = command {
                state.error = None;
                handle_command(&tx_screen_command, &mut state, &config, &mqtt, command).await;
                send_state(&mqtt, &state, topic_substr);
                psr.save(&state).unwrap_or_else(|e| {
                    error!("Failed to save state: {}", e);
                });
            } else {
                error!("Got unexpected audio command: {command:?}");
                state.error = Some(format!("Unexpected command: {command:?}"));
                state.play_list = None;
            }
        }
    });
}

async fn init_all(state: &State) -> Result<(), String> {
    init().await?;

    set_volume(state.volume.music).await?;
    if let Some(play_list) = &state.play_list {
        play_music(play_list).await?;
    } else {
        stop_music().await?;
    }
    Ok(())
}

fn send_state(mqtt: &MqttTx, state: &State, topic_substr: &str) {
    let topic = format!("state/{topic_substr}");
    match serde_json::to_string(&state) {
        Ok(json) => {
            let msg = MqttMessage::new(topic, json, true, QoS::AtLeastOnce);
            mqtt.try_send(msg);
        }
        Err(e) => {
            error!("Failed to serialize power state: {}", e);
        }
    }
}

fn send_task(mqtt: &MqttTx, task: &Task) {
    for message in get_task_messages(task) {
        debug!("Sending task {message:?}");
        mqtt.try_send(message.clone());
    }
}

async fn handle_command(
    tx_screen_command: &mpsc::Sender<ScreenCommand>,
    state: &mut State,
    config: &Arc<Config>,
    mqtt: &MqttTx,
    command: AudioCommand,
) {
    let music_volume = command.volume.as_ref().and_then(|v| v.music);
    let message_volume = command.volume.as_ref().and_then(|v| v.message);

    if let Some(music_volume) = music_volume {
        state.volume.music = music_volume;
    }

    if let Some(message_volume) = message_volume {
        state.volume.message = message_volume;
    }

    if let Some(message) = &command.message {
        let title = command
            .title
            .clone()
            .unwrap_or_else(|| "No title".to_string());
        let message = message.clone();
        tx_screen_command
            .try_send(ScreenCommand::Message { title, message })
            .unwrap_or_else(|err| {
                error!("Failed to send message to screen: {err}");
            });
    }

    let pre_tasks = command.pre_tasks.clone().unwrap_or_default();
    let post_tasks = command.post_tasks.clone().unwrap_or_default();

    for task in pre_tasks {
        let task = task.to_task(&config.targets);
        send_task(mqtt, &task);
    }

    process_command(state, command).await;

    for task in post_tasks {
        let task = task.to_task(&config.targets);
        send_task(mqtt, &task);
    }
}

enum Action {
    Sound(String),
    Say(String),
    Play(String),
    Stop,
}

impl Action {
    async fn execute(self, state: &State) -> Result<(), String> {
        match self {
            Self::Sound(sound) => {
                set_volume(state.volume.message).await?;
                play_sound(&sound).await?;
            }
            Self::Say(msg) => {
                set_volume(state.volume.message).await?;
                say(&msg).await?;
            }
            Self::Play(play_list) => {
                set_volume(state.volume.music).await?;
                play_music(&play_list).await?;
            }
            Self::Stop => {
                stop_music().await?;
            }
        }
        Ok(())
    }
}

fn get_actions_for_command(command: AudioCommand) -> Vec<Action> {
    let mut actions = Vec::new();

    if let Some(sound) = command.sound {
        actions.push(Action::Sound(sound));
    }

    if let Some(say) = command.message {
        actions.push(Action::Say(say));
    }

    if let Some(music) = command.music {
        if let Some(play_list) = music.play_list {
            actions.push(Action::Play(play_list));
        }

        if music.stop == Some(true) {
            actions.push(Action::Stop);
        }
    }

    actions
}

async fn process_command(state: &mut State, command: AudioCommand) {
    let play_list = command.music.clone().and_then(|m| m.play_list);

    let actions = get_actions_for_command(command);

    if actions.is_empty() {
        set_volume(state.volume.music).await.unwrap_or_else(|e| {
            state.error = Some(e);
            state.play_list = None;
        });
    } else {
        let play_action = actions
            .iter()
            .any(|a| matches!(a, Action::Play(..) | Action::Stop));

        let do_actions = || async {
            let paused = is_music_paused().await?;

            for action in actions {
                action.execute(state).await?;
            }

            if paused && !play_action {
                set_volume(state.volume.music).await?;
                music_resume().await?;
            } else if !paused && !play_action {
                set_volume(state.volume.music).await?;
            }

            state.play_list = play_list;
            Ok(())
        };

        do_actions().await.unwrap_or_else(|e| {
            state.error = Some(e);
            state.play_list = None;
        });
    }
}

async fn set_volume(volume: u8) -> Result<(), String> {
    let cl = Line::new(
        "amixer",
        vec!["set".into(), "Speaker".into(), format!("{volume}%")],
    );

    if let Err(err) = cl.run().await {
        error!("Failed to set volume: {err}");
        return Err(format!("Failed to set volume: {err}"));
    };

    Ok(())
}

async fn is_music_paused() -> Result<bool, String> {
    let cl = Line::new("mpc", vec!["pause-if-playing"]);

    match cl.run().await {
        Ok(_output) => Ok(true),
        Err(Error {
            kind: ErrorKind::BadExitCode,
            ..
        }) => Ok(false),
        Err(err) => {
            error!("Failed to get mpc status: {err}");
            Err(format!("Failed to get mpc status: {err}"))
        }
    }
}

async fn music_resume() -> Result<(), String> {
    let cl = Line::new("mpc", vec!["play"]);

    if let Err(err) = cl.run().await {
        error!("Failed to resume music: {err}");
        return Err(format!("Failed to resume music: {err}"));
    };
    Ok(())
}

async fn play_sound(sound: &str) -> Result<(), String> {
    let path = Path::new(sound)
        .file_name()
        .ok_or_else(|| format!("Failed to get file name from sound path: {sound}"))?;

    let path = Path::new("sounds")
        .join(path)
        .as_os_str()
        .to_str()
        .ok_or_else(|| {
            format!(
                "Failed to convert path to string: {}",
                path.to_string_lossy()
            )
        })?
        .to_string();

    let cl = Line::new("aplay", vec!["-q".into(), path]);

    if let Err(err) = cl.run().await {
        error!("Failed to play sound: {err}");
        return Err(format!("Failed to play sound: {err}"));
    };
    Ok(())
}

async fn say(message: &str) -> Result<(), String> {
    let cl = Line::new(
        "espeak",
        vec!["-v", "en-us", "-s", "150", "-a", "200", message],
    );

    play_sound("start.wav").await?;

    if let Err(err) = cl.run().await {
        error!("Failed to say message: {err}");
        return Err(format!("Failed to say message: {err}"));
    };

    play_sound("middle.wav").await?;

    if let Err(err) = cl.run().await {
        error!("Failed to say message: {err}");
        return Err(format!("Failed to say message: {err}"));
    };

    play_sound("stop.wav").await?;
    Ok(())
}

async fn play_music(play_list: &str) -> Result<(), String> {
    let cl_list = vec![
        Line::new("mpc", vec!["clear"]),
        Line::new("mpc", vec!["load", play_list]),
        Line::new("mpc", vec!["play"]),
    ];

    for cl in cl_list {
        if let Err(err) = cl.run().await {
            error!("Failed to play music: {err}");
            return Err(format!("Failed to play music: {err}"));
        };
    }

    Ok(())
}

async fn stop_music() -> Result<(), String> {
    let cl = Line::new("mpc", vec!["stop"]);

    if let Err(err) = cl.run().await {
        error!("Failed to stop music: {err}");
        return Err(format!("Failed to stop music: {err}"));
    };
    Ok(())
}

async fn init() -> Result<(), String> {
    let cl = Line::new("mpc", vec!["repeat", "on"]);
    if let Err(err) = cl.run().await {
        error!("Failed to init music: {err}");
        return Err(format!("Failed to init music: {err}"));
    };
    Ok(())
}
