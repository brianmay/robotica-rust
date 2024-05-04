//! Audio player service

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use robotica_backend::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::{
        mqtt::{MqttTx, Subscriptions},
        persistent_state::PersistentStateDatabase,
    },
    spawn,
};
use robotica_common::{
    mqtt::{Json, MqttMessage, QoS, Retain},
    robotica::{
        audio::{AudioCommand, Message, State},
        commands::Command,
        lights::LightCommand,
        switch::DevicePower,
        tasks::{Payload, SubTask, Task},
    },
};
use serde::Deserialize;
use tokio::{select, sync::mpsc};
use tracing::{debug, error, info};

use crate::{
    command::{self, ErrorKind},
    partial_command::{self, PartialLine},
    ui::ScreenCommand,
};

#[derive(Deserialize)]
pub struct ProgramsConfig {
    init: Vec<String>,
    set_volume: Vec<String>,
    pre_say: Vec<String>,
    say: Vec<String>,
    play_sound: Vec<String>,
    mpc: Vec<String>,
}

#[derive(Deserialize)]
pub struct Config {
    programs: ProgramsConfig,
    topic_substr: String,
    targets: HashMap<String, String>,
    messages_enabled_topic: String,
    sound_path: PathBuf,
}

#[derive()]
pub struct LoadedProgramsConfig {
    init: PartialLine,
    set_volume: PartialLine,
    pre_say: PartialLine,
    say: PartialLine,
    play_sound: PartialLine,
    mpc: PartialLine,
}

#[derive()]
pub struct LoadedConfig {
    programs: LoadedProgramsConfig,
    topic_substr: String,
    targets: HashMap<String, String>,
    messages_enabled_topic: String,
    sound_path: PathBuf,
}

impl TryFrom<Config> for LoadedConfig {
    type Error = partial_command::Error;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        let programs = LoadedProgramsConfig {
            init: PartialLine::new(config.programs.init)?,
            set_volume: PartialLine::new(config.programs.set_volume)?,
            pre_say: PartialLine::new(config.programs.pre_say)?,
            say: PartialLine::new(config.programs.say)?,
            play_sound: PartialLine::new(config.programs.play_sound)?,
            mpc: PartialLine::new(config.programs.mpc)?,
        };
        Ok(Self {
            programs,
            topic_substr: config.topic_substr,
            targets: config.targets,
            messages_enabled_topic: config.messages_enabled_topic,
            sound_path: config.sound_path,
        })
    }
}

pub fn run(
    tx_screen_command: mpsc::Sender<ScreenCommand>,
    subscriptions: &mut Subscriptions,
    mqtt: MqttTx,
    database: &PersistentStateDatabase,
    config: Arc<LoadedConfig>,
) {
    let topic_substr = &config.topic_substr;
    let topic = format!("command/{topic_substr}");
    let command_rx: stateless::Receiver<Json<Command>> =
        subscriptions.subscribe_into_stateless(topic);
    let messages_enabled_rx: stateful::Receiver<DevicePower> =
        subscriptions.subscribe_into_stateful(&config.messages_enabled_topic);
    let psr = database.for_name::<State>(topic_substr);
    let mut state = psr.load().unwrap_or_default();

    spawn(async move {
        let topic_substr = &config.topic_substr;

        let mut command_s = command_rx.subscribe().await;
        let mut messages_enabled_s = messages_enabled_rx.subscribe().await;
        let mut messages_enabled = false;

        init_all(&state, &config).await.unwrap_or_else(|err| {
            state.error = Some(err);
            state.play_list = None;
        });
        send_state(&mqtt, &state, topic_substr);

        #[allow(clippy::match_same_arms)]
        loop {
            select! {
                Ok(Json(command)) = command_s.recv() => {
                    if let Command::Audio(command) = command {
                        state.error = None;
                        handle_command(&tx_screen_command, &mut state, &config, &mqtt, command, messages_enabled).await;
                        send_state(&mqtt, &state, topic_substr);
                        psr.save(&state).unwrap_or_else(|e| {
                            error!("Failed to save state: {}", e);
                        });
                    } else if let Command::Message(command) = command {
                        let pre_tasks = if command.flash_lights {
                            vec![SubTask{
                                title: "Flash lights".to_string(),
                                target: "light".to_string(),
                                payload: Payload::Command(Command::Light(LightCommand::Flash)),
                                qos: QoS::ExactlyOnce,
                                retain: Retain::NoRetain,
                            }]
                        } else {
                            vec![]
                        };
                        let command = AudioCommand {
                            priority: command.priority,
                            sound: None,
                            pre_tasks: Some(pre_tasks),
                            post_tasks: None,
                            message: Some(Message {
                                title: command.title,
                                body: command.body,
                            }),
                            music: None,
                            volume: None,
                        };
                        state.error = None;
                        handle_command(&tx_screen_command, &mut state, &config, &mqtt, command, messages_enabled).await;
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
                Ok(me) = messages_enabled_s.recv() => {
                    messages_enabled = match me {
                        DevicePower::On => true,
                        DevicePower::Off => false,
                        DevicePower::AutoOff => false,
                        DevicePower::HardOff => false,
                        DevicePower::DeviceError => false,
                    };
                }
                else => break,
            }
        }
    });
}

async fn init_all(state: &State, config: &LoadedConfig) -> Result<(), String> {
    init(&config.programs).await?;

    set_volume(state.volume.music, &config.programs).await?;
    if let Some(play_list) = &state.play_list {
        play_music(play_list, &config.programs).await?;
    } else {
        stop_music(&config.programs).await?;
    }
    Ok(())
}

fn send_state(mqtt: &MqttTx, state: &State, topic_substr: &str) {
    let topic = format!("state/{topic_substr}");
    match serde_json::to_string(&state) {
        Ok(json) => {
            let msg = MqttMessage::new(topic, json, Retain::Retain, QoS::AtLeastOnce);
            mqtt.try_send(msg);
        }
        Err(e) => {
            error!("Failed to serialize power state: {}", e);
        }
    }
}

fn send_task(mqtt: &MqttTx, task: &Task) {
    for message in task.get_mqtt_messages() {
        debug!("Sending task {message:?}");
        mqtt.try_send(message.clone());
    }
}

async fn handle_command(
    tx_screen_command: &mpsc::Sender<ScreenCommand>,
    state: &mut State,
    config: &Arc<LoadedConfig>,
    mqtt: &MqttTx,
    command: AudioCommand,
    messages_enabled: bool,
) {
    let music_volume = command.volume.as_ref().and_then(|v| v.music);
    let message_volume = command.volume.as_ref().and_then(|v| v.message);

    if let Some(music_volume) = music_volume {
        state.volume.music = music_volume;
    }

    if let Some(message_volume) = message_volume {
        state.volume.message = message_volume;
    }

    if let Some(msg) = &command.message {
        tx_screen_command
            .try_send(ScreenCommand::Message(msg.clone()))
            .unwrap_or_else(|err| {
                error!("Failed to send message to screen: {err}");
            });
    }

    let should_play = {
        let now = chrono::Local::now();
        command.should_play(now, messages_enabled)
    };

    if should_play {
        process_command(state, command, config, mqtt).await;
    } else {
        info!("Not processing command due to lack of urgency: {command:?}");
    }
}

enum Action<'a> {
    Sound(&'a String),
    Say(&'a String),
    Play(&'a String),
    Tasks(&'a Vec<SubTask>),
    Stop,
}

impl<'a> Action<'a> {
    async fn execute(
        self,
        state: &State,
        config: &LoadedConfig,
        mqtt: &MqttTx,
    ) -> Result<(), String> {
        match self {
            Self::Sound(sound) => {
                set_volume(state.volume.message, &config.programs).await?;
                play_sound(sound, &config.programs, &config.sound_path).await?;
            }
            Self::Say(msg) => {
                set_volume(state.volume.message, &config.programs).await?;
                say(msg, &config.programs).await?;
            }
            Self::Play(play_list) => {
                set_volume(state.volume.music, &config.programs).await?;
                play_music(play_list, &config.programs).await?;
            }
            Self::Stop => {
                stop_music(&config.programs).await?;
            }
            Self::Tasks(tasks) => {
                for task in tasks {
                    let task = task.clone().to_task(&config.targets);
                    send_task(mqtt, &task);
                }
            }
        }
        Ok(())
    }
}

fn get_actions_for_command(command: &AudioCommand) -> Vec<Action> {
    let mut actions = Vec::new();

    if let Some(tasks) = &command.pre_tasks {
        actions.push(Action::Tasks(tasks));
    }

    if let Some(sound) = &command.sound {
        actions.push(Action::Sound(sound));
    }

    if let Some(msg) = &command.message {
        actions.push(Action::Say(&msg.body));
    }

    if let Some(music) = &command.music {
        if let Some(play_list) = &music.play_list {
            actions.push(Action::Play(play_list));
        }

        if music.stop == Some(true) {
            actions.push(Action::Stop);
        }
    }

    if let Some(tasks) = &command.post_tasks {
        actions.push(Action::Tasks(tasks));
    }

    actions
}

async fn process_command(
    state: &mut State,
    command: AudioCommand,
    config: &LoadedConfig,
    mqtt: &MqttTx,
) {
    let actions = get_actions_for_command(&command);

    if actions.is_empty() {
        set_volume(state.volume.music, &config.programs)
            .await
            .unwrap_or_else(|e| {
                state.error = Some(e);
                state.play_list = None;
            });
    } else {
        if let Some(music) = &command.music {
            state.play_list.clone_from(&music.play_list);
        };

        let play_action = actions
            .iter()
            .any(|a| matches!(a, Action::Play(..) | Action::Stop));

        let do_actions = || async {
            if let Some(msg) = &command.message {
                // If any errors occur in pre_say, they should already have been logged.
                // Just ignore them. The say call should still work regardless.
                _ = pre_say(&msg.body, &config.programs).await;
            }

            let paused = is_music_paused(&config.programs).await?;

            for action in actions {
                action.execute(state, config, mqtt).await?;
            }

            if paused && !play_action {
                set_volume(state.volume.music, &config.programs).await?;
                music_resume(&config.programs).await?;
            } else if !paused && !play_action {
                set_volume(state.volume.music, &config.programs).await?;
            }

            Ok(())
        };

        do_actions().await.unwrap_or_else(|e| {
            state.error = Some(e);
            state.play_list = None;
        });
    }
}

async fn set_volume(volume: u8, programs: &LoadedProgramsConfig) -> Result<(), String> {
    let cl = programs.set_volume.to_line_with_arg(format!("{volume}%"));
    if let Err(err) = cl.run().await {
        error!("Failed to set volume: {err}");
        return Err(format!("Failed to set volume: {err}"));
    };

    Ok(())
}

async fn is_music_paused(programs: &LoadedProgramsConfig) -> Result<bool, String> {
    let cl = programs.mpc.to_line_with_arg("pause-if-playing");
    match cl.run().await {
        Ok(_output) => Ok(true),
        Err(command::Error {
            kind: ErrorKind::BadExitCode { .. },
            ..
        }) => Ok(false),
        Err(err) => {
            error!("Failed to get mpc status: {err}");
            Err(format!("Failed to get mpc status: {err}"))
        }
    }
}

async fn music_resume(programs: &LoadedProgramsConfig) -> Result<(), String> {
    let cl = programs.mpc.to_line_with_arg("play");
    if let Err(err) = cl.run().await {
        error!("Failed to resume music: {err}");
        return Err(format!("Failed to resume music: {err}"));
    };
    Ok(())
}

async fn play_sound(
    sound: &str,
    programs: &LoadedProgramsConfig,
    sound_path: &Path,
) -> Result<(), String> {
    let path = Path::new(sound)
        .file_name()
        .ok_or_else(|| format!("Failed to get file name from sound path: {sound}"))?;

    let path = sound_path.join(path).as_os_str().to_owned();
    let cl = programs.play_sound.to_line_with_arg(path);

    if let Err(err) = cl.run().await {
        error!("Failed to play sound: {err}");
        return Err(format!("Failed to play sound: {err}"));
    };
    Ok(())
}
async fn pre_say(message: &str, programs: &LoadedProgramsConfig) -> Result<(), String> {
    let cl = programs.pre_say.to_line_with_arg(message);
    if let Err(err) = cl.run().await {
        error!("Failed to pre say message: {err}");
        return Err(format!("Failed to pre say message: {err}"));
    };
    Ok(())
}

async fn say(message: &str, programs: &LoadedProgramsConfig) -> Result<(), String> {
    let cl = programs.say.to_line_with_arg(message);
    if let Err(err) = cl.run().await {
        error!("Failed to say message: {err}");
        return Err(format!("Failed to say message: {err}"));
    };
    Ok(())
}

async fn play_music(play_list: &str, programs: &LoadedProgramsConfig) -> Result<(), String> {
    let cl_list = vec![
        programs.mpc.to_line_with_arg("clear"),
        programs.mpc.to_line_with_args(["load", play_list]),
        programs.mpc.to_line_with_arg("play"),
    ];

    for cl in cl_list {
        if let Err(err) = cl.run().await {
            error!("Failed to play music: {err}");
            return Err(format!("Failed to play music: {err}"));
        };
    }

    Ok(())
}

async fn stop_music(programs: &LoadedProgramsConfig) -> Result<(), String> {
    let cl = programs.mpc.to_line_with_arg("stop");
    if let Err(err) = cl.run().await {
        error!("Failed to stop music: {err}");
        return Err(format!("Failed to stop music: {err}"));
    };
    Ok(())
}

async fn init(programs: &LoadedProgramsConfig) -> Result<(), String> {
    let cl = programs.init.to_line();
    if let Err(err) = cl.run().await {
        error!("Failed to init music: {err}");
        return Err(format!("Failed to init music: {err}"));
    };
    Ok(())
}
