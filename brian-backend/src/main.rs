//! Main entry point for the application.

#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod amber;
mod delays;
mod environment_monitor;
mod ha;
mod hdmi;
mod lights;
mod robotica;
mod tesla;

use anyhow::Result;
use ha::MessageCommand;
use lights::{run_auto_light, run_passage_light, SharedEntities};
use robotica_backend::devices::lifx::DiscoverConfig;
use robotica_backend::devices::{fake_switch, lifx};
use robotica_backend::pipes::stateless;
use robotica_backend::scheduling::executor::executor;
use robotica_backend::services::persistent_state::PersistentStateDatabase;
use robotica_common::config::{
    ButtonConfig, ButtonRowConfig, ControllerConfig, Icon, RoomConfig, Rooms,
};
use robotica_common::controllers::robotica::lights2 as c_lights;
use robotica_common::controllers::robotica::music2 as c_music;
use robotica_common::controllers::robotica::{hdmi as c_hdmi, switch};
use robotica_common::controllers::{tasmota, zwave, Action};
use tracing::{debug, info};

use self::tesla::monitor_charging;
use robotica_backend::services::http;
use robotica_backend::services::mqtt::MqttTx;
use robotica_backend::services::mqtt::{mqtt_channel, run_client, Subscriptions};

#[allow(unreachable_code)]
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();

    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();
    let message_sink = ha::create_message_sink(mqtt.clone());
    let persistent_state_database = PersistentStateDatabase::new().unwrap_or_else(|e| {
        panic!("Error getting persistent state loader: {e}");
    });

    let mut state = State {
        subscriptions,
        mqtt,
        message_sink,
        persistent_state_database,
    };

    setup_pipes(&mut state).await;
    run_client(state.subscriptions, mqtt_rx)?;

    loop {
        debug!("I haven't crashed yet!");
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }

    Ok(())
}

/// Global state for the application.
pub struct State {
    subscriptions: Subscriptions,
    #[allow(dead_code)]
    mqtt: MqttTx,
    message_sink: stateless::Sender<MessageCommand>,
    persistent_state_database: PersistentStateDatabase,
}

fn get_rooms() -> Rooms {
    let passage_light = LightConfig {
        title: "Passage Light".to_string(),
        topic_substr: "Passage/Light".to_string(),
        ..Default::default()
    };

    let hdmi_inputs = vec![
        HdmiInputConfig {
            id: 1,
            title: "WiiU".to_string(),
        },
        HdmiInputConfig {
            id: 2,
            title: "Google".to_string(),
        },
        HdmiInputConfig {
            id: 3,
            title: "XBox".to_string(),
        },
        HdmiInputConfig {
            id: 4,
            title: "MythTV".to_string(),
        },
    ];

    let rooms: Rooms = vec![
        RoomConfig {
            id: "brian".to_string(),
            title: "Brian's Room".to_string(),
            menu: "Bedrooms".to_string(),
            rows: brian_config(&passage_light).into(),
        },
        RoomConfig {
            id: "jan".to_string(),
            title: "Jan's Room".to_string(),
            menu: "Bedrooms".to_string(),
            rows: jan_config(&passage_light).into(),
        },
        RoomConfig {
            id: "twins".to_string(),
            title: "Twins' Room".to_string(),
            menu: "Bedrooms".to_string(),
            rows: twins_config().into(),
        },
        RoomConfig {
            id: "dining".to_string(),
            title: "Dining Room".to_string(),
            menu: "Common".to_string(),
            rows: dining_config(&passage_light, &hdmi_inputs).into(),
        },
    ];

    rooms
}

fn brian_config(passage_light: &LightConfig) -> UiConfig {
    UiConfig {
        lights: vec![
            LightConfig {
                title: "Brian's Light".to_string(),
                topic_substr: "Brian/Light".to_string(),
                ..Default::default()
            },
            passage_light.clone(),
        ],
        music: vec![MusicConfig {
            title: "Brian's Music".to_string(),
            topic_substr: "Brian/Robotica".to_string(),
            extra_play_lists: vec![
                PlaylistConfig {
                    id: "sleep".to_string(),
                    title: "Sleep".to_string(),
                },
                PlaylistConfig {
                    id: "wake_up".to_string(),
                    title: "Wake Up".to_string(),
                },
            ],
            ..Default::default()
        }],
        switches: vec![ButtonRowConfig {
            id: "switches".to_string(),
            title: "Switches".to_string(),
            buttons: vec![
                ButtonConfig {
                    id: "fan".to_string(),
                    title: "Fan".to_string(),
                    icon: Icon::Fan,
                    controller: ControllerConfig::Zwave(zwave::Config {
                        action: Action::Toggle,
                        topic_substr: "Brians_Bedroom/Fan".to_string(),
                    }),
                },
                ButtonConfig {
                    id: "msg".to_string(),
                    title: "MSG".to_string(),
                    icon: Icon::Trumpet,
                    controller: ControllerConfig::Switch(switch::Config {
                        action: Action::Toggle,
                        topic_substr: "Brian/Messages".to_string(),
                    }),
                },
                ButtonConfig {
                    id: "bathroom".to_string(),
                    title: "Bathroom".to_string(),
                    icon: Icon::Trumpet,
                    controller: ControllerConfig::Switch(switch::Config {
                        action: Action::Toggle,
                        topic_substr: "Brian/Request_Bathroom".to_string(),
                    }),
                },
                ButtonConfig {
                    id: "night".to_string(),
                    title: "Night".to_string(),
                    icon: Icon::Night,
                    controller: ControllerConfig::Switch(switch::Config {
                        action: Action::Toggle,
                        topic_substr: "Brian/Night".to_string(),
                    }),
                },
            ],
        }],
        hdmis: vec![],
    }
}

fn jan_config(passage_light: &LightConfig) -> UiConfig {
    UiConfig {
        lights: vec![
            LightConfig {
                title: "Jan's Light".to_string(),
                topic_substr: "Jan/Light".to_string(),
                ..Default::default()
            },
            passage_light.clone(),
        ],
        music: vec![MusicConfig {
            title: "Jan's Music".to_string(),
            topic_substr: "Jan/Robotica".to_string(),
            extra_play_lists: vec![PlaylistConfig {
                id: "wake_up".to_string(),
                title: "Wake Up".to_string(),
            }],
            ..Default::default()
        }],
        switches: vec![],
        hdmis: vec![],
    }
}

fn twins_config() -> UiConfig {
    UiConfig {
        lights: vec![LightConfig {
            title: "Twins' Light".to_string(),
            topic_substr: "Twins/Light".to_string(),
            extra_scenes: vec![
                SceneConfig {
                    id: "declan-night".to_string(),
                    title: "Declan".to_string(),
                },
                SceneConfig {
                    id: "nikolai-night".to_string(),
                    title: "Nikolai".to_string(),
                },
            ],
            ..Default::default()
        }],
        music: vec![MusicConfig {
            title: "Twins' Music".to_string(),
            topic_substr: "Twins/Robotica".to_string(),
            extra_play_lists: vec![PlaylistConfig {
                id: "wake_up".to_string(),
                title: "Wake Up".to_string(),
            }],
            ..Default::default()
        }],
        switches: vec![],
        hdmis: vec![],
    }
}

fn dining_config(passage_light: &LightConfig, hdmi_inputs: &[HdmiInputConfig]) -> UiConfig {
    UiConfig {
        lights: vec![
            LightConfig {
                title: "Dining Light".to_string(),
                topic_substr: "Dining/Light".to_string(),
                ..Default::default()
            },
            passage_light.clone(),
        ],
        music: vec![MusicConfig {
            title: "Dining Music".to_string(),
            topic_substr: "Dining/Robotica".to_string(),
            ..Default::default()
        }],
        switches: vec![ButtonRowConfig {
            id: "switches".to_string(),
            title: "Switches".to_string(),
            buttons: vec![
                ButtonConfig {
                    id: "tv".to_string(),
                    title: "TV".to_string(),
                    icon: Icon::Tv,
                    controller: ControllerConfig::Tasmota(tasmota::Config {
                        action: Action::Toggle,
                        topic_substr: "tasmota_31E56F".to_string(),
                        power_postfix: String::new(),
                    }),
                },
                ButtonConfig {
                    id: "msg".to_string(),
                    title: "MSG".to_string(),
                    icon: Icon::Trumpet,
                    controller: ControllerConfig::Switch(switch::Config {
                        action: Action::Toggle,
                        topic_substr: "Dining/Messages".to_string(),
                    }),
                },
                ButtonConfig {
                    id: "bathroom".to_string(),
                    title: "Bathroom".to_string(),
                    icon: Icon::Trumpet,
                    controller: ControllerConfig::Switch(switch::Config {
                        action: Action::Toggle,
                        topic_substr: "Dining/Request_Bathroom".to_string(),
                    }),
                },
            ],
        }],
        hdmis: vec![HdmiConfig {
            title: "Dining TV".to_string(),
            topic_substr: "Dining/TV".to_string(),
            output_id: 1,
            inputs: hdmi_inputs.to_vec(),
        }],
    }
}

async fn setup_pipes(state: &mut State) {
    let price_summary_rx = amber::run(state).unwrap_or_else(|e| {
        panic!("Error running amber: {e}");
    });

    price_summary_rx
        .clone()
        .map(|(_, current)| current.is_cheap_2hr)
        .for_each(move |(old, current)| {
            if old.is_some() {
                let message = if current {
                    "2 hour cheap price has started"
                } else {
                    "2 hour cheap price has ended"
                };
                info!("{}", message);
            }
        });

    monitor_charging(state, 1, price_summary_rx).unwrap_or_else(|e| {
        panic!("Error running tesla charging monitor: {e}");
    });

    let rooms = get_rooms();
    http::run(state.mqtt.clone(), rooms)
        .await
        .unwrap_or_else(|e| panic!("Error running http server: {e}"));

    hdmi::run(state, "Dining", "TV", "hdmi.pri:8000");
    tesla::monitor_tesla_location(state, 1);
    tesla::monitor_tesla_doors(state, 1);

    environment_monitor::run(state).unwrap_or_else(|err| {
        panic!("Environment monitor failed: {err}");
    });

    executor(&mut state.subscriptions, state.mqtt.clone()).unwrap_or_else(|err| {
        panic!("Failed to start executor: {err}");
    });

    fake_switch(state, "Dining/Messages");
    fake_switch(state, "Dining/Request_Bathroom");

    fake_switch(state, "Brian/Night");
    fake_switch(state, "Brian/Messages");
    fake_switch(state, "Brian/Request_Bathroom");

    fake_switch(state, "Jan/Messages");

    fake_switch(state, "Twins/Messages");

    fake_switch(state, "Extension/Messages");

    fake_switch(state, "Akira/Messages");

    setup_lights(state).await;

    // let message_sink_temp = state.message_sink.clone();
    // let rx = state
    //     .subscriptions
    //     .subscribe_into::<Power>("state/Brian/Light/power");
    // spawn(async move {
    //     let mut s = rx.subscribe().await;
    //     loop {
    //         let msg = s.recv().await;
    //         if let Ok((Some(prev), current)) = msg {
    //             let announce = format!("Light power changed from {} to {}", prev, current);

    //             if let Err(err) = message_sink_temp.send(announce).await {
    //                 error!("Error sending message: {}", err);
    //             }
    //         }
    //         if let Some(msg) = rx.get().await {
    //             debug!("get: {:?}", msg);
    //         }
    //     }
    // });
}

fn fake_switch(state: &mut State, topic_substr: &str) {
    fake_switch::run(&mut state.subscriptions, state.mqtt.clone(), topic_substr);
}

#[derive(Clone)]
struct SceneConfig {
    id: String,
    title: String,
}

struct PlaylistConfig {
    id: String,
    title: String,
}

#[derive(Clone)]
struct HdmiInputConfig {
    id: u8,
    title: String,
}

#[derive(Clone)]
struct LightConfig {
    title: String,
    topic_substr: String,
    scenes: Vec<SceneConfig>,
    extra_scenes: Vec<SceneConfig>,
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            title: "No Title".to_string(),
            topic_substr: String::new(),
            scenes: vec![
                SceneConfig {
                    id: "auto".to_string(),
                    title: "Auto".to_string(),
                },
                SceneConfig {
                    id: "on".to_string(),
                    title: "On".to_string(),
                },
                SceneConfig {
                    id: "rainbow".to_string(),
                    title: "Rainbow".to_string(),
                },
            ],
            extra_scenes: vec![],
        }
    }
}

struct MusicConfig {
    title: String,
    topic_substr: String,
    play_lists: Vec<PlaylistConfig>,
    extra_play_lists: Vec<PlaylistConfig>,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self {
            title: "No Title".to_string(),
            topic_substr: String::new(),
            play_lists: vec![
                PlaylistConfig {
                    id: "stargate".to_string(),
                    title: "Stargate".to_string(),
                },
                PlaylistConfig {
                    id: "startrek".to_string(),
                    title: "Startrek".to_string(),
                },
                PlaylistConfig {
                    id: "frozen".to_string(),
                    title: "Frozen".to_string(),
                },
                PlaylistConfig {
                    id: "dragon".to_string(),
                    title: "Dragon".to_string(),
                },
            ],
            extra_play_lists: vec![],
        }
    }
}

struct HdmiConfig {
    title: String,
    topic_substr: String,
    output_id: u8,
    inputs: Vec<HdmiInputConfig>,
}

struct UiConfig {
    lights: Vec<LightConfig>,
    music: Vec<MusicConfig>,
    switches: Vec<ButtonRowConfig>,
    hdmis: Vec<HdmiConfig>,
}

impl From<LightConfig> for ButtonRowConfig {
    fn from(config: LightConfig) -> Self {
        let buttons = config
            .scenes
            .into_iter()
            .map(|scene| ButtonConfig {
                id: scene.id.clone(),
                title: scene.title,
                icon: Icon::Light,
                controller: ControllerConfig::Light2(c_lights::Config {
                    action: Action::Toggle,
                    topic_substr: config.topic_substr.clone(),
                    scene: scene.id,
                }),
            })
            .collect();

        let mut row = Self {
            id: config.topic_substr.clone(),
            title: config.title.clone(),
            buttons,
        };

        for scene in config.extra_scenes {
            row.buttons.push(ButtonConfig {
                id: scene.id.clone(),
                title: scene.title.clone(),
                icon: Icon::Light,
                controller: ControllerConfig::Light2(c_lights::Config {
                    action: Action::Toggle,
                    topic_substr: config.topic_substr.clone(),
                    scene: scene.id.clone(),
                }),
            });
        }

        row
    }
}

impl From<MusicConfig> for ButtonRowConfig {
    fn from(config: MusicConfig) -> Self {
        let buttons = config
            .play_lists
            .into_iter()
            .map(|play_list| ButtonConfig {
                id: play_list.id.clone(),
                title: play_list.title,
                icon: Icon::Trumpet,
                controller: ControllerConfig::Music2(c_music::Config {
                    action: Action::Toggle,
                    topic_substr: config.topic_substr.clone(),
                    play_list: play_list.id,
                }),
            })
            .collect();

        let mut row = Self {
            id: config.topic_substr.clone(),
            title: config.title.clone(),
            buttons,
        };

        for play_list in config.extra_play_lists {
            row.buttons.push(ButtonConfig {
                id: play_list.id.clone(),
                title: play_list.title.clone(),
                icon: Icon::Trumpet,
                controller: ControllerConfig::Music2(c_music::Config {
                    action: Action::Toggle,
                    topic_substr: config.topic_substr.clone(),
                    play_list: play_list.id.clone(),
                }),
            });
        }

        row
    }
}

impl From<HdmiConfig> for ButtonRowConfig {
    fn from(config: HdmiConfig) -> Self {
        let buttons = config
            .inputs
            .into_iter()
            .map(|input| ButtonConfig {
                id: input.id.to_string(),
                title: input.title,
                icon: Icon::Tv,
                controller: ControllerConfig::Hdmi(c_hdmi::Config {
                    action: Action::Toggle,
                    topic_substr: config.topic_substr.clone(),
                    output: config.output_id,
                    input: input.id,
                }),
            })
            .collect();

        Self {
            id: config.topic_substr,
            title: config.title,
            buttons,
        }
    }
}

impl From<UiConfig> for Vec<ButtonRowConfig> {
    fn from(config: UiConfig) -> Self {
        let mut buttons = vec![];

        for light in config.lights {
            buttons.push(ButtonRowConfig::from(light));
        }

        for music in config.music {
            buttons.push(ButtonRowConfig::from(music));
        }

        for switch in config.switches {
            buttons.push(switch);
        }

        for hdmi in config.hdmis {
            buttons.push(ButtonRowConfig::from(hdmi));
        }

        buttons
    }
}

async fn setup_lights(state: &mut State) {
    let lifx_config = DiscoverConfig {
        broadcast: "192.168.16.255:56700".to_string(),
        poll_time: std::time::Duration::from_secs(10),
        device_timeout: std::time::Duration::from_secs(45),
        api_timeout: std::time::Duration::from_secs(1),
        num_retries: 3,
    };
    let discover = lifx::discover(lifx_config)
        .await
        .unwrap_or_else(|e| panic!("Error discovering lifx devices: {e}"));

    let shared = SharedEntities::default();
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Brian/Light",
        105_867_434_619_856,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Dining/Light",
        74_174_870_942_672,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Jan/Light",
        189_637_382_730_704,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Twins/Light",
        116_355_744_756_688,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Akira/Light",
        280_578_114_286_544,
    );

    run_passage_light(
        state,
        discover,
        shared,
        "Passage/Light",
        137_092_148_851_664,
    );
}
