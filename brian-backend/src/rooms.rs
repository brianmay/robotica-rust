use robotica_common::{
    config::{ButtonConfig, ButtonRowConfig, ControllerConfig, Icon, RoomConfig, Rooms},
    controllers::{
        robotica::lights2,
        robotica::switch,
        robotica::{hdmi, music2},
        tasmota, zwave, Action,
    },
};

pub fn get() -> Rooms {
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
            rows: dining_room_config(&passage_light, &hdmi_inputs).into(),
        },
        RoomConfig {
            id: "living".to_string(),
            title: "Living Room".to_string(),
            menu: "Common".to_string(),
            rows: living_room_config(&hdmi_inputs).into(),
        },
        RoomConfig {
            id: "akira".to_string(),
            title: "Akira's Room".to_string(),
            menu: "Bedrooms".to_string(),
            rows: akira_config().into(),
        },
        RoomConfig {
            id: "passage".to_string(),
            title: "Passage".to_string(),
            menu: "Common".to_string(),
            rows: passage_config(&passage_light).into(),
        },
        RoomConfig {
            id: "tesla".to_string(),
            title: "Tesla".to_string(),
            menu: "Common".to_string(),
            rows: tesla_config().into(),
        },
    ];

    rooms
}

fn brian_config(passage_light: &LightConfig) -> UiConfig {
    UiConfig {
        lights: vec![
            LightConfig {
                title: "Light".to_string(),
                topic_substr: "Brian/Light".to_string(),
                ..Default::default()
            },
            passage_light.clone(),
        ],
        music: vec![MusicConfig {
            title: "Music".to_string(),
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
        hdmi_matrix: vec![],
    }
}

fn jan_config(passage_light: &LightConfig) -> UiConfig {
    UiConfig {
        lights: vec![
            LightConfig {
                title: "Light".to_string(),
                topic_substr: "Jan/Light".to_string(),
                ..Default::default()
            },
            passage_light.clone(),
        ],
        music: vec![MusicConfig {
            title: "Music".to_string(),
            topic_substr: "Jan/Robotica".to_string(),
            extra_play_lists: vec![PlaylistConfig {
                id: "wake_up".to_string(),
                title: "Wake Up".to_string(),
            }],
            ..Default::default()
        }],
        switches: vec![],
        hdmi_matrix: vec![],
    }
}

fn twins_config() -> UiConfig {
    UiConfig {
        lights: vec![LightConfig {
            title: "Light".to_string(),
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
            title: "Music".to_string(),
            topic_substr: "Twins/Robotica".to_string(),
            extra_play_lists: vec![PlaylistConfig {
                id: "wake_up".to_string(),
                title: "Wake Up".to_string(),
            }],
            ..Default::default()
        }],
        switches: vec![],
        hdmi_matrix: vec![],
    }
}

fn dining_room_config(passage_light: &LightConfig, hdmi_inputs: &[HdmiInputConfig]) -> UiConfig {
    UiConfig {
        lights: vec![
            LightConfig {
                title: "Light".to_string(),
                topic_substr: "Dining/Light".to_string(),
                ..Default::default()
            },
            passage_light.clone(),
        ],
        music: vec![MusicConfig {
            title: "Music".to_string(),
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
        hdmi_matrix: vec![HdmiConfig {
            title: "TV".to_string(),
            topic_substr: "Dining/TV".to_string(),
            output_id: 1,
            inputs: hdmi_inputs.to_vec(),
        }],
    }
}

fn living_room_config(hdmi_inputs: &[HdmiInputConfig]) -> UiConfig {
    UiConfig {
        lights: vec![LightConfig {
            title: "Akira's Light".to_string(),
            topic_substr: "Akira/Light".to_string(),
            ..Default::default()
        }],
        music: vec![MusicConfig {
            title: "Music".to_string(),
            topic_substr: "Extension/Robotica".to_string(),
            ..Default::default()
        }],
        switches: vec![],
        hdmi_matrix: vec![HdmiConfig {
            title: "TV".to_string(),
            topic_substr: "Living/TV".to_string(),
            output_id: 1,
            inputs: hdmi_inputs.to_vec(),
        }],
    }
}

fn akira_config() -> UiConfig {
    UiConfig {
        lights: vec![LightConfig {
            title: "Light".to_string(),
            topic_substr: "Akira/Light".to_string(),
            ..Default::default()
        }],
        music: vec![MusicConfig {
            title: "Music".to_string(),
            topic_substr: "Extension/Robotica".to_string(),
            ..Default::default()
        }],
        switches: vec![],
        hdmi_matrix: vec![],
    }
}

fn passage_config(passage_light: &LightConfig) -> UiConfig {
    UiConfig {
        lights: vec![passage_light.clone()],
        music: vec![],
        switches: vec![],
        hdmi_matrix: vec![],
    }
}

fn tesla_config() -> UiConfig {
    UiConfig {
        lights: vec![],
        music: vec![],
        switches: vec![ButtonRowConfig {
            id: "switches".to_string(),
            title: "Switches".to_string(),
            buttons: vec![
                ButtonConfig {
                    id: "charge".to_string(),
                    title: "Charge".to_string(),
                    icon: Icon::Light,
                    controller: ControllerConfig::Switch(switch::Config {
                        action: Action::Toggle,
                        topic_substr: "Tesla/1/AutoCharge".to_string(),
                    }),
                },
                ButtonConfig {
                    id: "force".to_string(),
                    title: "Force".to_string(),
                    icon: Icon::Light,
                    controller: ControllerConfig::Switch(switch::Config {
                        action: Action::Toggle,
                        topic_substr: "Tesla/1/ForceCharge".to_string(),
                    }),
                },
            ],
        }],
        hdmi_matrix: vec![],
    }
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
                    id: "train_dragon".to_string(),
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
    hdmi_matrix: Vec<HdmiConfig>,
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
                controller: ControllerConfig::Light2(lights2::Config {
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
                controller: ControllerConfig::Light2(lights2::Config {
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
                controller: ControllerConfig::Music2(music2::Config {
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
                controller: ControllerConfig::Music2(music2::Config {
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
                controller: ControllerConfig::Hdmi(hdmi::Config {
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

        for hdmi in config.hdmi_matrix {
            buttons.push(ButtonRowConfig::from(hdmi));
        }

        buttons
    }
}
