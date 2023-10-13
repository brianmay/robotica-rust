use std::{
    iter::{empty, repeat, zip},
    str::FromStr,
    time::Duration,
};

use robotica_backend::{
    devices::lifx::{device_entity, Device, DeviceConfig},
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::{mqtt::MqttTx, persistent_state::PersistentStateRow},
    spawn,
};
use robotica_common::{
    mqtt::{Json, MqttMessage, QoS},
    robotica::{
        commands::Command,
        lights::{self, Colors, LightCommand, PowerColor, PowerLevel, PowerState, State, HSBK},
    },
};
use tokio::{select, time::sleep};
use tracing::{debug, error};

trait GetSceneEntity {
    type Scenes;
    fn get_scene_entity(&self, scene: Self::Scenes) -> stateful::Receiver<PowerColor>;
}

trait ScenesTrait: FromStr + ToString + Default {}

#[derive(Debug, Clone, Copy, Default)]
enum StandardScenes {
    On,
    Auto,
    Rainbow,
    Busy,
    AkiraNight,
    DeclanNight,
    NikolaiNight,
    #[default]
    Off,
}

impl FromStr for StandardScenes {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "on" => Ok(Self::On),
            "auto" => Ok(Self::Auto),
            "rainbow" => Ok(Self::Rainbow),
            "busy" => Ok(Self::Busy),
            "akira-night" => Ok(Self::AkiraNight),
            "declan-night" => Ok(Self::DeclanNight),
            "nikolai-night" => Ok(Self::NikolaiNight),
            "off" => Ok(Self::Off),
            _ => Err(()),
        }
    }
}

impl ToString for StandardScenes {
    fn to_string(&self) -> String {
        match self {
            Self::On => "on",
            Self::Auto => "auto",
            Self::Rainbow => "rainbow",
            Self::Busy => "busy",
            Self::AkiraNight => "akira-night",
            Self::DeclanNight => "declan-night",
            Self::NikolaiNight => "nikolai-night",
            Self::Off => "off",
        }
        .to_string()
    }
}

impl ScenesTrait for StandardScenes {}

#[derive(Clone)]
pub struct SharedEntities {
    on: stateful::Receiver<PowerColor>,
    rainbow: stateful::Receiver<PowerColor>,
    busy: stateful::Receiver<PowerColor>,
    akira_night: stateful::Receiver<PowerColor>,
    declan_night: stateful::Receiver<PowerColor>,
    nikolai_night: stateful::Receiver<PowerColor>,
    off: stateful::Receiver<PowerColor>,
}

impl Default for SharedEntities {
    fn default() -> Self {
        let on_color = Colors::Single(HSBK {
            hue: 0.0,
            saturation: 0.0,
            brightness: 100.0,
            kelvin: 3500,
        });

        let akira_night_color = Colors::Single(HSBK {
            hue: 240.0,
            saturation: 100.0,
            brightness: 6.0,
            kelvin: 3500,
        });

        let declan_night_color = Colors::Single(HSBK {
            hue: 52.0,
            saturation: 50.0,
            brightness: 6.0,
            kelvin: 3500,
        });

        let nikolai_night_color = Colors::Single(HSBK {
            hue: 261.0,
            saturation: 100.0,
            brightness: 6.0,
            kelvin: 3500,
        });

        Self {
            on: static_entity(PowerColor::On(on_color), "On"),
            rainbow: rainbow_entity("rainbow"),
            busy: busy_entity("busy"),
            akira_night: static_entity(PowerColor::On(akira_night_color), "akira-night"),
            declan_night: static_entity(PowerColor::On(declan_night_color), "akira-night"),
            nikolai_night: static_entity(PowerColor::On(nikolai_night_color), "akira-night"),
            off: static_entity(PowerColor::Off, "off"),
        }
    }
}

#[derive(Clone)]
struct StandardSceneEntities {
    on: stateful::Receiver<PowerColor>,
    auto: stateful::Receiver<PowerColor>,
    rainbow: stateful::Receiver<PowerColor>,
    busy: stateful::Receiver<PowerColor>,
    akira_night: stateful::Receiver<PowerColor>,
    declan_night: stateful::Receiver<PowerColor>,
    nikolai_night: stateful::Receiver<PowerColor>,
    off: stateful::Receiver<PowerColor>,
}

impl StandardSceneEntities {
    fn default(state: &mut crate::InitState, shared: SharedEntities, topic_substr: &str) -> Self {
        Self {
            on: shared.on,
            auto: mqtt_entity(state, topic_substr, "auto"),
            rainbow: shared.rainbow,
            busy: shared.busy,
            akira_night: shared.akira_night,
            declan_night: shared.declan_night,
            nikolai_night: shared.nikolai_night,
            off: shared.off,
        }
    }
}

const fn flash_color() -> PowerColor {
    PowerColor::On(Colors::Single(HSBK {
        hue: 240.0,
        saturation: 50.0,
        brightness: 100.0,
        kelvin: 3500,
    }))
}

impl GetSceneEntity for StandardSceneEntities {
    type Scenes = StandardScenes;

    fn get_scene_entity(&self, scene: Self::Scenes) -> stateful::Receiver<PowerColor> {
        match scene {
            StandardScenes::On => self.on.clone(),
            StandardScenes::Auto => self.auto.clone(),
            StandardScenes::Rainbow => self.rainbow.clone(),
            StandardScenes::Busy => self.busy.clone(),
            StandardScenes::AkiraNight => self.akira_night.clone(),
            StandardScenes::DeclanNight => self.declan_night.clone(),
            StandardScenes::NikolaiNight => self.nikolai_night.clone(),
            StandardScenes::Off => self.off.clone(),
        }
    }
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

fn mqtt_entity(
    state: &mut crate::InitState,
    topic_substr: &str,
    name: impl Into<String>,
) -> stateful::Receiver<PowerColor> {
    let name = name.into();
    let topic: String = format!("command/{topic_substr}/{name}");

    let pc_rx = state
        .subscriptions
        .subscribe_into_stateful::<Json<PowerColor>>(&topic)
        .map(|(_, Json(c))| c);

    let (tx, rx) = stateful::create_pipe(name);
    spawn(async move {
        let mut pc_s = pc_rx.subscribe().await;
        loop {
            select! {
                Ok(pc) = pc_s.recv() => {
                    tx.try_send(pc);
                }
                () = tx.closed() => {
                    break;
                }
            }
        }
    });
    rx
}

pub fn run_auto_light(
    state: &mut crate::InitState,
    discover: stateless::Receiver<Device>,
    shared: SharedEntities,
    topic_substr: &str,
    id: u64,
) {
    let entities = StandardSceneEntities::default(state, shared, topic_substr);
    let (tx_state, rx_state) = stateful::create_pipe(format!("{id}-state"));
    let rx = switch_entity(
        state,
        entities,
        topic_substr,
        flash_color(),
        format!("{id}_switch"),
    );

    run_state_sender(state, topic_substr, rx_state);
    device_entity(rx, tx_state, id, discover, DeviceConfig::default());
}

fn run_state_sender(
    state: &crate::InitState,
    topic_substr: impl Into<String>,
    rx_state: stateful::Receiver<State>,
) {
    let topic_substr = topic_substr.into();

    {
        let mqtt = state.mqtt.clone();
        let topic_substr = topic_substr.to_string();
        let rx = rx_state.clone();
        spawn(async move {
            let mut rx = rx.subscribe().await;
            while let Ok(status) = rx.recv().await {
                send_state(&mqtt, &status, &topic_substr);
            }
        });
    }

    {
        let mqtt = state.mqtt.clone();
        let rx = rx_state.map(|(_, status)| match status {
            lights::State::Online(PowerColor::On(..)) => lights::PowerState::On,
            lights::State::Online(PowerColor::Off) => lights::PowerState::Off,
            lights::State::Offline => lights::PowerState::Offline,
        });
        spawn(async move {
            let mut rx = rx.subscribe().await;
            while let Ok(status) = rx.recv().await {
                send_power_state(&mqtt, &status, &topic_substr);
            }
        });
    }
}

pub fn run_passage_light(
    state: &mut crate::InitState,
    discover: stateless::Receiver<Device>,
    shared: SharedEntities,
    topic_substr: &str,
    id: u64,
) {
    let (tx_state, rx_state) = stateful::create_pipe(format!("{id}-state"));

    let all_topic_substr = topic_substr.to_string();
    let cupboard_topic_substr = format!("{topic_substr}/split/cupboard");
    let bathroom_topic_substr = format!("{topic_substr}/split/bathroom");
    let bedroom_topic_substr = format!("{topic_substr}/split/bedroom");

    let switch_entities = StandardSceneEntities::default(state, shared, topic_substr);
    let entities = PassageEntities {
        all: switch_entity(
            state,
            switch_entities.clone(),
            all_topic_substr.clone(),
            flash_color(),
            format!("{id}-all"),
        ),
        cupboard: switch_entity(
            state,
            switch_entities.clone(),
            cupboard_topic_substr.clone(),
            flash_color(),
            format!("{id}-cupboard"),
        ),
        bathroom: switch_entity(
            state,
            switch_entities.clone(),
            bathroom_topic_substr.clone(),
            flash_color(),
            format!("{id}-bathroom"),
        ),
        bedroom: switch_entity(
            state,
            switch_entities,
            bedroom_topic_substr.clone(),
            flash_color(),
            format!("{id}-bedroom"),
        ),
    };

    let config = DeviceConfig {
        multiple_zones: true,
    };

    let (rx, state_entities) =
        run_passage_multiplexer(entities, format!("{id}-multiplexer"), rx_state);

    run_state_sender(state, all_topic_substr, state_entities.all);
    run_state_sender(state, cupboard_topic_substr, state_entities.cupboard);
    run_state_sender(state, bathroom_topic_substr, state_entities.bathroom);
    run_state_sender(state, bedroom_topic_substr, state_entities.bedroom);

    device_entity(rx, tx_state, id, discover, config);
}

struct LightState<Entities>
where
    Entities: GetSceneEntity + Send + Sync + 'static,
    Entities::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entities::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    scene: Entities::Scenes,
    entities: Entities,
    entity: stateful::Receiver<PowerColor>,
    entity_s: stateful::Subscription<PowerColor>,
    psr: PersistentStateRow<String>,
    mqtt: MqttTx,
    topic_substr: String,
    tx: stateful::Sender<PowerColor>,
    flash_color: PowerColor,
}

fn switch_entity<Entities>(
    state: &mut crate::InitState,
    entities: Entities,
    topic_substr: impl Into<String>,
    flash_color: PowerColor,
    name: impl Into<String>,
) -> stateful::Receiver<PowerColor>
where
    Entities: GetSceneEntity + Send + Sync + 'static,
    Entities::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entities::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    let (tx, rx) = stateful::create_pipe(name);

    let topic_substr: String = topic_substr.into();
    let topic = &format!("command/{topic_substr}");
    let rx_command = state
        .subscriptions
        .subscribe_into_stateless::<Json<Command>>(topic);

    {
        let psr = state.persistent_state_database.for_name(&topic_substr);
        let scene: String = psr.load().unwrap_or_default();
        let scene = Entities::Scenes::from_str(&scene).unwrap_or_default();

        let mqtt = state.mqtt.clone();
        let topic_substr: String = topic_substr;
        spawn(async move {
            let mut state = {
                let entity = entities.get_scene_entity(scene);
                let entity_s = entity.subscribe().await;

                LightState {
                    scene,
                    entities,
                    entity,
                    entity_s,
                    psr,
                    mqtt,
                    topic_substr,
                    tx,
                    flash_color,
                }
            };

            let mut rx_command_s = rx_command.subscribe().await;

            state
                .psr
                .save(&scene.to_string())
                .unwrap_or_else(|e| error!("Failed to save scene: {}", e));
            send_scene(&state.mqtt, &scene, &state.topic_substr);

            loop {
                tokio::select! {
                    Ok(Json(command)) = rx_command_s.recv() => {
                        debug!("Got command: {:?}", command);
                        match command {
                            Command::Light(command) => {
                                process_command(&mut state, command).await;
                            }
                            _ => {
                                error!("Invalid command, expected light, got {:?}", command);
                            }
                        }
                    }
                    Ok(pc) = state.entity_s.recv() => {
                        state.tx.try_send(pc);
                    }
                }
            }
        });
    }

    rx
}

async fn process_command<Entity>(state: &mut LightState<Entity>, command: LightCommand)
where
    Entity: GetSceneEntity + Send + Sync + 'static,
    Entity::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entity::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    match command {
        LightCommand::TurnOn { scene } => {
            if let Ok(scene) = Entity::Scenes::from_str(&scene) {
                set_scene(state, scene).await;
            } else {
                error!("Invalid scene: {}", scene);
            }
        }
        LightCommand::TurnOff => {
            let scene = Entity::Scenes::default();
            set_scene(state, scene).await;
        }

        LightCommand::Flash => {
            let pc = (state.entity.get().await).map_or_else(|| PowerColor::Off, |pc| pc);
            state.tx.try_send(state.flash_color.clone());
            sleep(Duration::from_millis(500)).await;
            state.tx.try_send(pc.clone());
            sleep(Duration::from_millis(500)).await;
            state.tx.try_send(state.flash_color.clone());
            sleep(Duration::from_millis(500)).await;
            state.tx.try_send(pc);
        }
    }
}

async fn set_scene<Entity>(
    state: &mut LightState<Entity>,
    scene: <Entity as GetSceneEntity>::Scenes,
) where
    Entity: GetSceneEntity + Send + Sync + 'static,
    Entity::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entity::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    state.scene = scene;
    state.entity = state.entities.get_scene_entity(scene);
    state
        .psr
        .save(&scene.to_string())
        .unwrap_or_else(|e| error!("Failed to save scene: {}", e));
    send_scene(&state.mqtt, &scene, &state.topic_substr);
    state.tx.try_send(PowerColor::Off);
    state.entity_s = state.entity.subscribe().await;
}

fn send_state(mqtt: &MqttTx, state: &lights::State, topic_substr: &str) {
    let topic = format!("state/{topic_substr}/status");
    match serde_json::to_string(&state) {
        Ok(json) => {
            let msg = MqttMessage::new(topic, json, true, QoS::AtLeastOnce);
            mqtt.try_send(msg);
        }
        Err(e) => {
            error!("Failed to serialize status: {}", e);
        }
    }
}

fn send_power_state(mqtt: &MqttTx, power_state: &PowerState, topic_substr: &str) {
    let topic = format!("state/{topic_substr}/power");
    match serde_json::to_string(&power_state) {
        Ok(json) => {
            let msg = MqttMessage::new(topic, json, true, QoS::AtLeastOnce);
            mqtt.try_send(msg);
        }
        Err(e) => {
            error!("Failed to serialize power status: {}", e);
        }
    }
}

fn send_scene<Scene: ScenesTrait>(mqtt: &MqttTx, scene: &Scene, topic_substr: &str) {
    let topic = format!("state/{topic_substr}/scene");
    let msg = MqttMessage::new(topic, scene.to_string(), true, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}

struct PassageEntities {
    all: stateful::Receiver<PowerColor>,
    cupboard: stateful::Receiver<PowerColor>,
    bathroom: stateful::Receiver<PowerColor>,
    bedroom: stateful::Receiver<PowerColor>,
}

struct PassageStateEntities {
    all: stateful::Receiver<State>,
    cupboard: stateful::Receiver<State>,
    bathroom: stateful::Receiver<State>,
    bedroom: stateful::Receiver<State>,
}

fn run_passage_multiplexer(
    entities: PassageEntities,
    name: impl Into<String>,
    state_in: stateful::Receiver<State>,
) -> (stateful::Receiver<PowerColor>, PassageStateEntities) {
    let name = name.into();
    let (tx, rx) = stateful::create_pipe(name.clone());
    let (tx_all_state, rx_all_state) = stateful::create_pipe(format!("{name}-all"));
    let (tx_cupboard_state, rx_cupboard_state) = stateful::create_pipe(format!("{name}-cupboard"));
    let (tx_bathroom_state, rx_bathroom_state) = stateful::create_pipe(format!("{name}-bathroom"));
    let (tx_bedroom_state, rx_bedroom_state) = stateful::create_pipe(format!("{name}-bathroom"));

    spawn(async move {
        let mut all = entities.all.subscribe().await;
        let mut cupboard = entities.cupboard.subscribe().await;
        let mut bathroom = entities.bathroom.subscribe().await;
        let mut bedroom = entities.bedroom.subscribe().await;
        let mut state_s = state_in.subscribe().await;

        let mut all_colors = PowerColor::Off;
        let mut cupboard_colors = PowerColor::Off;
        let mut bathroom_colors = PowerColor::Off;
        let mut bedroom_colors = PowerColor::Off;

        let mut state = None;

        loop {
            tokio::select! {
                Ok(pc) = all.recv() => {
                    all_colors = pc;
                }
                Ok(pc) = cupboard.recv() => {
                    cupboard_colors = pc;
                }
                Ok(pc) = bathroom.recv() => {
                    bathroom_colors = pc;
                }
                Ok(pc) = bedroom.recv() => {
                    bedroom_colors = pc;
                }
                Ok(s) = state_s.recv() => {
                    state = Some(s);
                }
            }

            match state {
                None => {}
                Some(State::Offline) => {
                    tx_all_state.try_send(State::Offline);
                    tx_cupboard_state.try_send(State::Offline);
                    tx_bathroom_state.try_send(State::Offline);
                    tx_bedroom_state.try_send(State::Offline);
                }
                Some(_) => {
                    tx_all_state.try_send(State::Online(all_colors.clone()));
                    tx_cupboard_state.try_send(State::Online(cupboard_colors.clone()));
                    tx_bathroom_state.try_send(State::Online(bathroom_colors.clone()));
                    tx_bedroom_state.try_send(State::Online(bedroom_colors.clone()));
                }
            }

            let power = match (
                &all_colors,
                &cupboard_colors,
                &bathroom_colors,
                &bedroom_colors,
            ) {
                (PowerColor::Off, PowerColor::Off, PowerColor::Off, PowerColor::Off) => {
                    PowerLevel::Off
                }
                _ => PowerLevel::On,
            };

            let mut colors = Vec::with_capacity(32);
            for _ in 0..32 {
                colors.push(HSBK {
                    hue: 0.0,
                    saturation: 0.0,
                    brightness: 0.0,
                    kelvin: 3500,
                });
            }

            copy_colors_to_pos(&all_colors, &mut colors, 0, 32);
            copy_colors_to_pos(&cupboard_colors, &mut colors, 7, 7);
            copy_colors_to_pos(&bathroom_colors, &mut colors, 23, 7);
            copy_colors_to_pos(&bedroom_colors, &mut colors, 30, 2);

            let pc = match power {
                PowerLevel::On => PowerColor::On(Colors::Sequence(colors)),
                PowerLevel::Off => PowerColor::Off,
            };

            tx.try_send(pc);
        }
    });

    let pse = PassageStateEntities {
        all: rx_all_state,
        cupboard: rx_cupboard_state,
        bathroom: rx_bathroom_state,
        bedroom: rx_bedroom_state,
    };

    (rx, pse)
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
