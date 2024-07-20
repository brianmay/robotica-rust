use std::{
    collections::HashMap,
    iter::{empty, repeat, zip},
    time::Duration,
};

use robotica_backend::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::persistent_state::PersistentStateRow,
    spawn,
};
use robotica_common::{
    mqtt::Json,
    robotica::{
        commands::Command,
        lights::{Colors, LightCommand, PowerColor, PowerLevel, SceneName, HSBK},
    },
};
use tokio::time::sleep;
use tracing::{debug, error};

#[derive(Debug, Clone)]
pub struct Scene {
    rx: stateful::Receiver<PowerColor>,
    name: SceneName,
}

impl Scene {
    pub const fn new(rx: stateful::Receiver<PowerColor>, name: SceneName) -> Self {
        Self { rx, name }
    }
}

impl Default for Scene {
    fn default() -> Self {
        let name = SceneName::default();
        let rx = static_entity(PowerColor::Off, "default");
        Self { rx, name }
    }
}

#[derive(Clone, Debug)]
pub struct SceneMap(HashMap<SceneName, Scene>);

impl SceneMap {
    pub const fn new(scenes: HashMap<SceneName, Scene>) -> Self {
        Self(scenes)
    }

    pub fn get(&self, name: &SceneName) -> Option<&Scene> {
        self.0.get(name)
    }

    pub fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    pub fn insert(&mut self, name: SceneName, scene: Scene) {
        self.0.insert(name, scene);
    }
}

// #[derive(Clone)]
// pub struct SharedEntities {
//     on: stateful::Receiver<PowerColor>,
//     rainbow: stateful::Receiver<PowerColor>,
//     busy: stateful::Receiver<PowerColor>,
// akira_night: stateful::Receiver<PowerColor>,
// declan_night: stateful::Receiver<PowerColor>,
// nikolai_night: stateful::Receiver<PowerColor>,
//     off: stateful::Receiver<PowerColor>,
// }

// impl Default for SharedEntities {
pub fn get_default_scenes() -> SceneMap {
    let on_color = Colors::Single(HSBK {
        hue: 0.0,
        saturation: 0.0,
        brightness: 100.0,
        kelvin: 3500,
    });

    // let akira_night_color = Colors::Single(HSBK {
    //     hue: 240.0,
    //     saturation: 100.0,
    //     brightness: 6.0,
    //     kelvin: 3500,
    // });

    // let declan_night_color = Colors::Single(HSBK {
    //     hue: 52.0,
    //     saturation: 50.0,
    //     brightness: 6.0,
    //     kelvin: 3500,
    // });

    // let nikolai_night_color = Colors::Single(HSBK {
    //     hue: 261.0,
    //     saturation: 100.0,
    //     brightness: 6.0,
    //     kelvin: 3500,
    // });

    let mut map = HashMap::new();
    map.insert(
        SceneName::new("on"),
        Scene::new(
            static_entity(PowerColor::On(on_color), "On"),
            SceneName::new("on"),
        ),
    );

    map.insert(
        SceneName::new("rainbow"),
        Scene::new(rainbow_entity("rainbow"), SceneName::new("rainbow")),
    );

    map.insert(
        SceneName::new("busy"),
        Scene::new(busy_entity("busy"), SceneName::new("busy")),
    );

    map.insert(
        SceneName::new("off"),
        Scene::new(static_entity(PowerColor::Off, "off"), SceneName::new("off")),
    );

    // Self {
    //     on: static_entity(PowerColor::On(on_color), "On"),
    //     rainbow: rainbow_entity("rainbow"),
    //     busy: busy_entity("busy"),
    //     // akira_night: static_entity(PowerColor::On(akira_night_color), "akira-night"),
    //     // declan_night: static_entity(PowerColor::On(declan_night_color), "akira-night"),
    //     // nikolai_night: static_entity(PowerColor::On(nikolai_night_color), "akira-night"),
    //     off: static_entity(PowerColor::Off, "off"),
    // }

    SceneMap::new(map)
}
// }

// #[derive(Clone)]
// struct StandardSceneEntities {
//     on: stateful::Receiver<PowerColor>,
//     auto: stateful::Receiver<PowerColor>,
//     rainbow: stateful::Receiver<PowerColor>,
//     busy: stateful::Receiver<PowerColor>,
//     akira_night: stateful::Receiver<PowerColor>,
//     declan_night: stateful::Receiver<PowerColor>,
//     nikolai_night: stateful::Receiver<PowerColor>,
//     off: stateful::Receiver<PowerColor>,
// }

// impl StandardSceneEntities {
//     fn default(auto: stateful::Receiver<PowerColor>, shared: &SharedEntities) -> Self {
//         Self {
//             on: shared.on.clone(),
//             auto,
//             rainbow: shared.rainbow.clone(),
//             busy: shared.busy.clone(),
//             akira_night: shared.akira_night.clone(),
//             declan_night: shared.declan_night.clone(),
//             nikolai_night: shared.nikolai_night.clone(),
//             off: shared.off.clone(),
//         }
//     }
// }

// const fn flash_color() -> PowerColor {
//     PowerColor::On(Colors::Single(HSBK {
//         hue: 240.0,
//         saturation: 50.0,
//         brightness: 100.0,
//         kelvin: 3500,
//     }))
// }

// impl GetSceneEntity for StandardSceneEntities {
//     type Scene = StandardScene;

//     fn get_scene_entity(&self, scene: Self::Scene) -> stateful::Receiver<PowerColor> {
//         match scene {
//             StandardScene::On => self.on.clone(),
//             StandardScene::Auto => self.auto.clone(),
//             StandardScene::Rainbow => self.rainbow.clone(),
//             StandardScene::Busy => self.busy.clone(),
//             StandardScene::AkiraNight => self.akira_night.clone(),
//             StandardScene::DeclanNight => self.declan_night.clone(),
//             StandardScene::NikolaiNight => self.nikolai_night.clone(),
//             StandardScene::Off => self.off.clone(),
//         }
//     }
// }

fn static_entity(pc: PowerColor, name: impl Into<String>) -> stateful::Receiver<PowerColor> {
    let (tx, rx) = stateful::create_pipe(name);
    spawn(async move {
        tx.try_send(pc);
        tx.closed().await;
    });
    rx
}

fn busy_entity(name: impl Into<String>) -> stateful::Receiver<PowerColor> {
    let (tx, rx) = stateful::create_pipe(name);
    spawn(async move {
        while !tx.is_closed() {
            let on_color = HSBK {
                hue: 0.0,
                saturation: 100.0,
                brightness: 100.0,
                kelvin: 3500,
            };

            let off_color = HSBK {
                hue: 0.0,
                saturation: 20.0,
                brightness: 0.0,
                kelvin: 3500,
            };

            let colors = Colors::Sequence(vec![on_color, off_color]);
            tx.try_send(PowerColor::On(colors));
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            let colors = Colors::Sequence(vec![off_color, on_color]);
            tx.try_send(PowerColor::On(colors));
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });
    rx
}

fn rainbow_entity(name: impl Into<String>) -> stateful::Receiver<PowerColor> {
    let (tx, rx) = stateful::create_pipe(name);
    spawn(async move {
        let mut i = 0u16;
        let num_per_cycle = 10u16;

        while !tx.is_closed() {
            let colors: Vec<HSBK> = (0..num_per_cycle)
                .map(|j| {
                    let mut hue = f32::from(i + j) * 360.0 / f32::from(num_per_cycle);
                    while hue >= 360.0 {
                        hue -= 360.0;
                    }
                    HSBK {
                        hue,
                        saturation: 100.0,
                        brightness: 100.0,
                        kelvin: 3500,
                    }
                })
                .collect();
            let colors = Colors::Sequence(colors);

            tx.try_send(PowerColor::On(colors));
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            i = (i + 1) % num_per_cycle;
        }
    });
    rx
}

// fn mqtt_entity(
//     state: &mut crate::InitState,
//     topic_substr: &str,
//     name: impl Into<String>,
// ) -> stateful::Receiver<PowerColor> {
//     let name = name.into();
//     let topic: String = format!("command/{topic_substr}/{name}");

//     let pc_rx = state
//         .subscriptions
//         .subscribe_into_stateful::<Json<PowerColor>>(&topic)
//         .map(|(_, Json(c))| c);

//     let (tx, rx) = stateful::create_pipe(name);
//     spawn(async move {
//         let mut pc_s = pc_rx.subscribe().await;
//         loop {
//             select! {
//                 Ok(pc) = pc_s.recv() => {
//                     tx.try_send(pc);
//                 }
//                 () = tx.closed() => {
//                     break;
//                 }
//             }
//         }
//     });
//     rx
// }

pub struct Inputs {
    pub commands: stateless::Receiver<Json<Command>>,
    // pub auto: stateful::Receiver<PowerColor>,
}

pub struct Outputs {
    // pub state: stateful::Receiver<State>,
    pub scene: stateful::Receiver<SceneName>,
    pub pc: stateful::Receiver<PowerColor>,
}

#[must_use]
pub fn run_auto_light(
    inputs: Inputs,
    persistent_state_database: &crate::PersistentStateDatabase,
    // discover: stateless::Receiver<Device>,
    scene_map: SceneMap,
    flash_color: PowerColor,
    id: &str,
) -> Outputs {
    // let (state_tx, state_rx) = stateful::create_pipe(format!("{lifx_id}-state"));
    let (pc_rx, scene_rx) = switch_entity(
        inputs.commands,
        persistent_state_database,
        id,
        scene_map,
        flash_color,
    );

    // device_entity(pc_rx, state_tx, lifx_id, discover, DeviceConfig::default());

    Outputs {
        // state: state_rx,
        pc: pc_rx,
        scene: scene_rx,
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct SplitPowerColor {
    priority: usize,
    pc: PowerColor,
}

pub struct SplitOutputs {
    pub spc: stateful::Receiver<SplitPowerColor>,
    pub scene: stateful::Receiver<SceneName>,
}

#[must_use]
pub fn run_split_light(
    inputs: Inputs,
    persistent_state_database: &crate::PersistentStateDatabase,
    // discover: stateless::Receiver<Device>,
    scene_map: SceneMap,
    flash_color: PowerColor,
    // id: impl Into<String>,
    id: &str,
    // lifx_id: LifxId,
    priority: usize,
) -> SplitOutputs {
    // let (state_tx, state_rx) = stateful::create_pipe(format!("{lifx_id}-state"));
    let (pc_rx, scene_rx) = switch_entity(
        inputs.commands,
        persistent_state_database,
        id,
        scene_map,
        flash_color,
    );

    let pc_rx = pc_rx.map(move |(_, pc)| SplitPowerColor { priority, pc });
    // let pc_rx = run_merge_light(pc_rx, lifx_id, DeviceConfig::default());

    SplitOutputs {
        // state: state_rx,
        scene: scene_rx,
        spc: pc_rx,
    }
}

pub struct SplitLightConfig {
    // pub priority: usize,
    pub begin: usize,
    pub number: usize,
}

pub struct MergeLightConfig {
    pub number_of_lights: usize,
    pub splits: Vec<SplitLightConfig>,
}

#[must_use]
pub fn run_merge_light(
    split_rx: stateful::Receiver<SplitPowerColor>,
    // discover: stateless::Receiver<Device>,
    // lifx_id: LifxId,
    id: &str,
    config: MergeLightConfig,
) -> stateful::Receiver<PowerColor> {
    let (merged_tx, merged_rx) = stateful::create_pipe(format!("{id}/merged"));
    // let (state_tx, state_rx) = stateful::create_pipe(format!("{lifx_id}-state"));

    spawn(async move {
        let mut rx = split_rx.subscribe().await;
        let len = config.splits.len();
        let mut buffers = Vec::with_capacity(len);
        for _ in 0..len {
            buffers.push(PowerColor::Off);
        }

        while let Ok(SplitPowerColor { priority, pc }) = rx.recv().await {
            if priority > len {
                error!("Priority out of range: {}", priority);
                continue;
            }

            buffers[priority] = pc;

            let mut power = PowerLevel::Off;
            let mut colors = Vec::with_capacity(config.number_of_lights);
            for _ in 0..32 {
                colors.push(HSBK {
                    hue: 0.0,
                    saturation: 0.0,
                    brightness: 0.0,
                    kelvin: 3500,
                });
            }

            for (i, buffer) in buffers.iter_mut().enumerate() {
                let begin = config.splits[i].begin;
                let number = config.splits[i].number;
                let split = buffer;
                copy_colors_to_pos(split, &mut colors, begin, number);
                if matches!(split, PowerColor::On(..)) {
                    power = PowerLevel::On;
                }
            }

            let pc = match power {
                PowerLevel::On => PowerColor::On(Colors::Sequence(colors)),
                PowerLevel::Off => PowerColor::Off,
            };

            merged_tx.try_send(pc);
        }
    });

    // device_entity(
    //     merged_rx,
    //     state_tx,
    //     lifx_id,
    //     discover,
    //     DeviceConfig::default(),
    // );

    merged_rx
}

// fn run_state_sender(
//     state: &crate::InitState,
//     topic_substr: impl Into<String>,
//     rx_state: stateful::Receiver<State>,
// ) {
//     let topic_substr = topic_substr.into();

//     {
//         let mqtt = state.mqtt.clone();
//         let topic_substr = topic_substr.to_string();
//         let rx = rx_state.clone();
//         spawn(async move {
//             let mut rx = rx.subscribe().await;
//             while let Ok(status) = rx.recv().await {
//                 send_state(&mqtt, &status, &topic_substr);
//             }
//         });
//     }

//     {
//         let mqtt = state.mqtt.clone();
//         let rx = rx_state.map(|(_, status)| match status {
//             lights::State::Online(PowerColor::On(..)) => lights::PowerState::On,
//             lights::State::Online(PowerColor::Off) => lights::PowerState::Off,
//             lights::State::Offline => lights::PowerState::Offline,
//         });
//         spawn(async move {
//             let mut rx = rx.subscribe().await;
//             while let Ok(status) = rx.recv().await {
//                 send_power_state(&mqtt, &status, &topic_substr);
//             }
//         });
//     }
// }

// pub struct PassageInputs {
//     pub all: Inputs,
//     pub cupboard: Inputs,
//     pub bathroom: Inputs,
//     pub bedroom: Inputs,
// }

// pub struct PassageOutputs {
//     pub all: Outputs,
//     pub cupboard: Outputs,
//     pub bathroom: Outputs,
//     pub bedroom: Outputs,
// }

// pub struct PassageScenes {
//     pub all: SceneMap,
//     pub cupboard: SceneMap,
//     pub bathroom: SceneMap,
//     pub bedroom: SceneMap,
// }

// #[must_use]
// pub fn run_passage_light(
//     inputs: PassageInputs,
//     persistent_state_database: &crate::PersistentStateDatabase,
//     discover: stateless::Receiver<Device>,
//     passage_scenes: PassageScenes,
//     id: &str,
//     lifx_id: u64,
// ) -> PassageOutputs {
//     let (tx_state, rx_state) = stateful::create_pipe(format!("{lifx_id}-state"));

//     let (all_pc_rx, all_scene_rx) = switch_entity(
//         inputs.all.commands,
//         persistent_state_database,
//         id,
//         passage_scenes.all,
//         flash_color(),
//         format!("{lifx_id}-all"),
//     );

//     let (cupboard_pc_rx, cupboard_scene_rx) = switch_entity(
//         inputs.cupboard.commands,
//         persistent_state_database,
//         id,
//         passage_scenes.cupboard,
//         flash_color(),
//         format!("{lifx_id}-cupboard"),
//     );

//     let (bathroom_pc_rx, bathroom_scene_rx) = switch_entity(
//         inputs.bathroom.commands,
//         persistent_state_database,
//         id,
//         passage_scenes.bathroom,
//         flash_color(),
//         format!("{lifx_id}-bathroom"),
//     );

//     let (bedroom_pc_rx, bedroom_scene_rx) = switch_entity(
//         inputs.bedroom.commands,
//         persistent_state_database,
//         id,
//         passage_scenes.bedroom,
//         flash_color(),
//         format!("{lifx_id}-bedroom"),
//     );

//     let entities = PassageEntities {
//         all: all_pc_rx,
//         cupboard: cupboard_pc_rx,
//         bathroom: bathroom_pc_rx,
//         bedroom: bedroom_pc_rx,
//     };

//     let config = DeviceConfig {
//         multiple_zones: true,
//     };

//     let (rx, state_entities) =
//         run_passage_multiplexer(entities, format!("{lifx_id}-multiplexer"), rx_state);

//     device_entity(rx, tx_state, lifx_id, discover, config);

//     PassageOutputs {
//         all: Outputs {
//             state: state_entities.all,
//             scene: all_scene_rx,
//         },

//         cupboard: Outputs {
//             state: state_entities.cupboard,
//             scene: cupboard_scene_rx,
//         },

//         bathroom: Outputs {
//             state: state_entities.bathroom,
//             scene: bathroom_scene_rx,
//         },

//         bedroom: Outputs {
//             state: state_entities.bedroom,
//             scene: bedroom_scene_rx,
//         },
//     }
// }

struct LightState {
    // scene_map: SceneMap,
    // scene: Scene,
    // scene_name: String,
    // entity: stateful::Receiver<PowerColor>,
    entity_s: stateful::Subscription<PowerColor>,
    psr: PersistentStateRow<SceneName>,
    // mqtt: MqttTx,
    // topic_substr: String,
    pc_tx: stateful::Sender<PowerColor>,
    scene_tx: stateful::Sender<SceneName>,
    flash_color: PowerColor,
    last_value: Option<PowerColor>,
}

fn switch_entity(
    rx_command: stateless::Receiver<Json<Command>>,
    persistent_state_database: &crate::PersistentStateDatabase,
    // topic_substr: impl Into<String>,
    // id: impl Into<String>,
    id: &str,
    scene_map: SceneMap,
    flash_color: PowerColor,
    // name: impl Into<String>,
) -> (
    stateful::Receiver<PowerColor>,
    stateful::Receiver<SceneName>,
) {
    // let name = name.into();
    // let id = id.into();
    let (pc_tx, pc_rx) = stateful::create_pipe(format!("{id}/pc"));
    let (scene_tx, scene_rx) = stateful::create_pipe(format!("{id}/scenes"));

    // let id: String = id.into();
    {
        let psr = persistent_state_database.for_name(id);
        let scene_name: SceneName = psr.load().unwrap_or_default();
        let scene = scene_map.get(&scene_name).cloned().unwrap_or_default();

        spawn(async move {
            let mut state = {
                let entity = scene.rx.clone();
                let entity_s = entity.subscribe().await;

                LightState {
                    // scene_map,
                    // scene,
                    // entity,
                    entity_s,
                    psr,
                    pc_tx,
                    scene_tx,
                    flash_color,
                    last_value: None,
                }
            };

            let mut rx_command_s = rx_command.subscribe().await;
            state.scene_tx.try_send(scene.name);

            loop {
                tokio::select! {
                    Ok(Json(command)) = rx_command_s.recv() => {
                        debug!("Got command: {:?}", command);
                        match command {
                            Command::Light(command) => {
                                process_command(&mut state, command, &scene_map).await;
                            }
                            _ => {
                                error!("Invalid command, expected light, got {:?}", command);
                            }
                        }
                    }
                    Ok(pc) = state.entity_s.recv() => {
                        state.last_value = Some(pc.clone());
                        state.pc_tx.try_send(pc);
                    }
                }
            }
        });
    }

    (pc_rx, scene_rx)
}

async fn process_command(state: &mut LightState, command: LightCommand, scene_map: &SceneMap) {
    match command {
        LightCommand::TurnOn { scene } => {
            if let Some(scene) = scene_map.get(&scene) {
                set_scene(state, scene).await;
            } else {
                error!("Invalid scene: {}", scene);
            }
        }
        LightCommand::TurnOff => {
            let scene_name = SceneName::new("off".to_string());
            if let Some(scene) = scene_map.get(&scene_name) {
                set_scene(state, scene).await;
            } else {
                error!("Invalid scene: {}", "off");
            }
        }

        LightCommand::Flash => {
            let pc = state.last_value.clone().unwrap_or(PowerColor::Off);
            state.pc_tx.try_send(state.flash_color.clone());
            sleep(Duration::from_millis(500)).await;
            state.pc_tx.try_send(pc.clone());
            sleep(Duration::from_millis(500)).await;
            state.pc_tx.try_send(state.flash_color.clone());
            sleep(Duration::from_millis(500)).await;
            state.pc_tx.try_send(pc);
        }
    }
}

async fn set_scene(state: &mut LightState, scene: &Scene) {
    // state.scene = scene;
    // state.entity = state.entities.get_scene_entity(scene);
    state
        .psr
        .save(&scene.name)
        .unwrap_or_else(|e| error!("Failed to save scene: {}", e));
    state.scene_tx.try_send(scene.name.clone());
    state.pc_tx.try_send(PowerColor::Off);
    state.entity_s = scene.rx.subscribe().await;
    state.last_value = None;
}

// fn send_state(mqtt: &MqttTx, state: &lights::State, topic_substr: &str) {
//     let topic = format!("state/{topic_substr}/status");
//     match serde_json::to_string(&state) {
//         Ok(json) => {
//             let msg = MqttMessage::new(topic, json, Retain::Retain, QoS::AtLeastOnce);
//             mqtt.try_send(msg);
//         }
//         Err(e) => {
//             error!("Failed to serialize status: {}", e);
//         }
//     }
// }

// fn send_power_state(mqtt: &MqttTx, power_state: &PowerState, topic_substr: &str) {
//     let topic = format!("state/{topic_substr}/power");
//     match serde_json::to_string(&power_state) {
//         Ok(json) => {
//             let msg = MqttMessage::new(topic, json, Retain::Retain, QoS::AtLeastOnce);
//             mqtt.try_send(msg);
//         }
//         Err(e) => {
//             error!("Failed to serialize power status: {}", e);
//         }
//     }
// }

// fn send_scene<Scene: ScenesTrait>(mqtt: &MqttTx, scene: &Scene, topic_substr: &str) {
//     let topic = format!("state/{topic_substr}/scene");
//     let msg = MqttMessage::new(topic, scene.to_string(), Retain::Retain, QoS::AtLeastOnce);
//     mqtt.try_send(msg);
// }

// struct PassageEntities {
//     all: stateful::Receiver<PowerColor>,
//     cupboard: stateful::Receiver<PowerColor>,
//     bathroom: stateful::Receiver<PowerColor>,
//     bedroom: stateful::Receiver<PowerColor>,
// }

// struct PassageStateEntities {
//     all: stateful::Receiver<State>,
//     cupboard: stateful::Receiver<State>,
//     bathroom: stateful::Receiver<State>,
//     bedroom: stateful::Receiver<State>,
// }

// fn run_passage_multiplexer(
//     entities: PassageEntities,
//     name: impl Into<String>,
//     state_in: stateful::Receiver<State>,
// ) -> (stateful::Receiver<PowerColor>, PassageStateEntities) {
//     let name = name.into();
//     let (tx, rx) = stateful::create_pipe(name.clone());
//     let (tx_all_state, rx_all_state) = stateful::create_pipe(format!("{name}-all"));
//     let (tx_cupboard_state, rx_cupboard_state) = stateful::create_pipe(format!("{name}-cupboard"));
//     let (tx_bathroom_state, rx_bathroom_state) = stateful::create_pipe(format!("{name}-bathroom"));
//     let (tx_bedroom_state, rx_bedroom_state) = stateful::create_pipe(format!("{name}-bathroom"));

//     spawn(async move {
//         let mut all = entities.all.subscribe().await;
//         let mut cupboard = entities.cupboard.subscribe().await;
//         let mut bathroom = entities.bathroom.subscribe().await;
//         let mut bedroom = entities.bedroom.subscribe().await;
//         let mut state_s = state_in.subscribe().await;

//         let mut all_colors = PowerColor::Off;
//         let mut cupboard_colors = PowerColor::Off;
//         let mut bathroom_colors = PowerColor::Off;
//         let mut bedroom_colors = PowerColor::Off;

//         let mut state = None;

//         loop {
//             tokio::select! {
//                 Ok(pc) = all.recv() => {
//                     all_colors = pc;
//                 }
//                 Ok(pc) = cupboard.recv() => {
//                     cupboard_colors = pc;
//                 }
//                 Ok(pc) = bathroom.recv() => {
//                     bathroom_colors = pc;
//                 }
//                 Ok(pc) = bedroom.recv() => {
//                     bedroom_colors = pc;
//                 }
//                 Ok(s) = state_s.recv() => {
//                     state = Some(s);
//                 }
//             }

//             match state {
//                 None => {}
//                 Some(State::Offline) => {
//                     tx_all_state.try_send(State::Offline);
//                     tx_cupboard_state.try_send(State::Offline);
//                     tx_bathroom_state.try_send(State::Offline);
//                     tx_bedroom_state.try_send(State::Offline);
//                 }
//                 Some(_) => {
//                     tx_all_state.try_send(State::Online(all_colors.clone()));
//                     tx_cupboard_state.try_send(State::Online(cupboard_colors.clone()));
//                     tx_bathroom_state.try_send(State::Online(bathroom_colors.clone()));
//                     tx_bedroom_state.try_send(State::Online(bedroom_colors.clone()));
//                 }
//             }

//             let power = match (
//                 &all_colors,
//                 &cupboard_colors,
//                 &bathroom_colors,
//                 &bedroom_colors,
//             ) {
//                 (PowerColor::Off, PowerColor::Off, PowerColor::Off, PowerColor::Off) => {
//                     PowerLevel::Off
//                 }
//                 _ => PowerLevel::On,
//             };

//             let mut colors = Vec::with_capacity(32);
//             for _ in 0..32 {
//                 colors.push(HSBK {
//                     hue: 0.0,
//                     saturation: 0.0,
//                     brightness: 0.0,
//                     kelvin: 3500,
//                 });
//             }

//             copy_colors_to_pos(&all_colors, &mut colors, 0, 32);
//             copy_colors_to_pos(&cupboard_colors, &mut colors, 7, 7);
//             copy_colors_to_pos(&bathroom_colors, &mut colors, 23, 7);
//             copy_colors_to_pos(&bedroom_colors, &mut colors, 30, 2);

//             let pc = match power {
//                 PowerLevel::On => PowerColor::On(Colors::Sequence(colors)),
//                 PowerLevel::Off => PowerColor::Off,
//             };

//             tx.try_send(pc);
//         }
//     });

//     let pse = PassageStateEntities {
//         all: rx_all_state,
//         cupboard: rx_cupboard_state,
//         bathroom: rx_bathroom_state,
//         bedroom: rx_bedroom_state,
//     };

//     (rx, pse)
// }

fn copy_colors_to_pos(add_colors: &PowerColor, colors: &mut [HSBK], offset: usize, number: usize) {
    let x: Box<dyn Iterator<Item = HSBK>> = match add_colors {
        PowerColor::On(Colors::Single(color)) => Box::new(repeat(*color).take(number)),
        PowerColor::On(Colors::Sequence(colors)) => {
            Box::new(colors.iter().copied().cycle().take(number))
        }
        PowerColor::Off => Box::new(empty()),
    };

    for (src, dst) in zip(x, colors.iter_mut().skip(offset)) {
        *dst = src;
    }
}
