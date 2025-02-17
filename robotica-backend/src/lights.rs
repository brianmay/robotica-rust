use std::{
    collections::HashMap,
    iter::{empty, repeat, zip},
    time::Duration,
};

use robotica_common::robotica::entities::Id;
use robotica_common::{
    mqtt::Json,
    robotica::{
        commands::Command,
        lights::{Colors, LightCommand, PowerColor, PowerLevel, SceneName, HSBK},
    },
};
use robotica_tokio::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::persistent_state::PersistentStateRow,
    spawn,
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

pub fn get_default_scenes() -> SceneMap {
    let on_color = Colors::Single(HSBK {
        hue: 0.0,
        saturation: 0.0,
        brightness: 100.0,
        kelvin: 3500,
    });

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

    SceneMap::new(map)
}

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
                    hue = hue.rem_euclid(360.0);
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
    id: &Id,
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
    id: &Id,
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
    id: &Id,
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
    merged_rx
}

struct LightState {
    entity_s: stateful::Subscription<PowerColor>,
    psr: PersistentStateRow<SceneName>,
    pc_tx: stateful::Sender<PowerColor>,
    scene_tx: stateful::Sender<SceneName>,
    flash_color: PowerColor,
    last_value: Option<PowerColor>,
}

fn switch_entity(
    rx_command: stateless::Receiver<Json<Command>>,
    persistent_state_database: &crate::PersistentStateDatabase,
    id: &Id,
    scene_map: SceneMap,
    flash_color: PowerColor,
) -> (
    stateful::Receiver<PowerColor>,
    stateful::Receiver<SceneName>,
) {
    let (pc_tx, pc_rx) = stateful::create_pipe(format!("{id}/pc"));
    let (scene_tx, scene_rx) = stateful::create_pipe(format!("{id}/scenes"));

    {
        let psr = persistent_state_database.for_name(id, "scene");
        let scene_name: SceneName = psr.load().unwrap_or_default();
        let scene = scene_map.get(&scene_name).cloned().unwrap_or_default();

        spawn(async move {
            let mut state = {
                let entity = scene.rx.clone();
                let entity_s = entity.subscribe().await;

                LightState {
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
