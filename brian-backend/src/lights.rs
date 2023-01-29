use std::{
    iter::{empty, repeat, zip},
    str::FromStr,
    time::Duration,
};

use log::{debug, error};
use robotica_backend::{
    devices::lifx::{device_entity, Device},
    entities::{self, create_stateless_entity, Receiver, Sender, StatefulData, Subscription},
    services::{mqtt::Mqtt, persistent_state::PersistentStateRow},
    spawn,
};
use robotica_common::{
    mqtt::{MqttMessage, QoS},
    robotica::{
        commands::{Command, Light2Command},
        lights::{self, Colors, PowerColor, PowerLevel, PowerState, State, HSBK},
    },
};
use tokio::time::sleep;

trait GetSceneEntity {
    type Scenes;
    fn get_scene_entity(&self, scene: Self::Scenes) -> Receiver<PowerColor>;
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
struct StandardSceneEntities {
    on: Receiver<PowerColor>,
    auto: Receiver<PowerColor>,
    rainbow: Receiver<PowerColor>,
    busy: Receiver<PowerColor>,
    akira_night: Receiver<PowerColor>,
    declan_night: Receiver<PowerColor>,
    nikolai_night: Receiver<PowerColor>,
    off: Receiver<PowerColor>,
}

impl StandardSceneEntities {
    fn default(state: &mut crate::State, topic_substr: &str) -> Self {
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
            auto: mqtt_entity(state, topic_substr, "auto"),
            rainbow: rainbow_entity("rainbow"),
            busy: busy_entity("busy"),
            akira_night: static_entity(PowerColor::On(akira_night_color), "akira-night"),
            declan_night: static_entity(PowerColor::On(declan_night_color), "akira-night"),
            nikolai_night: static_entity(PowerColor::On(nikolai_night_color), "akira-night"),
            off: static_entity(PowerColor::Off, "off"),
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

    fn get_scene_entity(&self, scene: Self::Scenes) -> Receiver<PowerColor> {
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

fn static_entity(pc: PowerColor, name: impl Into<String>) -> Receiver<PowerColor> {
    let (tx, rx) = entities::create_stateless_entity(name);
    tx.try_send(pc);
    rx
}

fn busy_entity(name: impl Into<String>) -> Receiver<PowerColor> {
    let (tx, rx) = entities::create_stateless_entity(name);
    spawn(async move {
        loop {
            let on_color = HSBK {
                hue: 0.0,
                saturation: 100.0,
                brightness: 100.0,
                kelvin: 3500,
            };

            let off_color = HSBK {
                hue: 0.0,
                saturation: 100.0,
                brightness: 0.0,
                kelvin: 3500,
            };

            let colors = Colors::Sequence(vec![on_color, off_color]);
            tx.try_send(PowerColor::On(colors));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let colors = Colors::Sequence(vec![off_color, on_color]);
            tx.try_send(PowerColor::On(colors));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    rx
}

fn rainbow_entity(name: impl Into<String>) -> Receiver<PowerColor> {
    let (tx, rx) = entities::create_stateless_entity(name);
    spawn(async move {
        let mut i = 0.0;
        loop {
            let color = Colors::Single(HSBK {
                hue: i,
                saturation: 100.0,
                brightness: 100.0,
                kelvin: 3500,
            });
            tx.try_send(PowerColor::On(color));
            i += 10.0;
            if i >= 360.0 {
                i = 0.0;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    rx
}

fn mqtt_entity(
    state: &mut crate::State,
    topic_substr: &str,
    name: impl Into<String>,
) -> Receiver<PowerColor> {
    let name = name.into();
    let topic: String = format!("command/{topic_substr}/{name}");

    let pc_rx = state
        .subscriptions
        .subscribe_into_stateless::<PowerColor>(&topic);

    let (tx, rx) = entities::create_stateless_entity(name);
    spawn(async move {
        let mut pc_s = pc_rx.subscribe().await;
        while let Ok(pc) = pc_s.recv().await {
            tx.try_send(pc);
        }
    });
    rx
}

pub fn run_auto_light(
    state: &mut crate::State,
    discover: Receiver<Device>,
    topic_substr: &str,
    id: u64,
) {
    let entities = StandardSceneEntities::default(state, topic_substr);
    let (tx_state, rx_state) = entities::create_stateful_entity(format!("{id}-state"));
    let rx = switch_entity(
        state,
        rx_state,
        entities,
        topic_substr,
        flash_color(),
        format!("{id}_switch"),
    );
    device_entity(rx, tx_state, id, discover);
}

pub fn run_passage_light(
    state: &mut crate::State,
    discover: Receiver<Device>,
    topic_substr: &str,
    id: u64,
) {
    let (tx_state, rx_state) = entities::create_stateful_entity(format!("{id}-state"));

    let switch_entities = StandardSceneEntities::default(state, topic_substr);
    let entities = PassageEntities {
        all: switch_entity(
            state,
            rx_state.clone(),
            switch_entities.clone(),
            topic_substr,
            flash_color(),
            format!("{id}_all"),
        ),
        cupboard: switch_entity(
            state,
            rx_state.clone(),
            switch_entities.clone(),
            format!("{topic_substr}/split/cupboard"),
            flash_color(),
            format!("{id}_cupboard"),
        ),
        bathroom: switch_entity(
            state,
            rx_state.clone(),
            switch_entities.clone(),
            format!("{topic_substr}/split/bathroom"),
            flash_color(),
            format!("{id}_bathroom"),
        ),
        bedroom: switch_entity(
            state,
            rx_state,
            switch_entities,
            format!("{topic_substr}/split/bedroom"),
            flash_color(),
            format!("{id}_bedroom"),
        ),
    };

    let rx = run_passage_multiplexer(entities, format!("{id}_multiplexer"));
    device_entity(rx, tx_state, id, discover);
}

struct LightState<Entities>
where
    Entities: GetSceneEntity + Send + Sync + 'static,
    Entities::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entities::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    scene: Entities::Scenes,
    entities: Entities,
    entity: Receiver<PowerColor>,
    entity_s: Subscription<PowerColor>,
    psr: PersistentStateRow<String>,
    mqtt: Mqtt,
    topic_substr: String,
    tx: Sender<PowerColor>,
    flash_color: PowerColor,
}

fn switch_entity<Entities>(
    state: &mut crate::State,
    rx_state: Receiver<StatefulData<State>>,
    entities: Entities,
    topic_substr: impl Into<String>,
    flash_color: PowerColor,
    name: impl Into<String>,
) -> Receiver<PowerColor>
where
    Entities: GetSceneEntity + Send + Sync + 'static,
    Entities::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entities::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    let (tx, rx) = entities::create_stateless_entity(name);

    let topic_substr: String = topic_substr.into();
    let topic = &format!("command/{topic_substr}");
    let rx_command = state
        .subscriptions
        .subscribe_into_stateless::<Command>(topic);

    {
        let mqtt = state.mqtt.clone();
        let topic_substr = topic_substr.to_string();
        let rx = rx_state.clone();
        spawn(async move {
            let mut rx = rx.subscribe().await;
            while let Ok((_, status)) = rx.recv().await {
                send_state(&mqtt, &status, &topic_substr);
            }
        });
    }

    {
        let mqtt = state.mqtt.clone();
        let topic_substr = topic_substr.to_string();
        let rx = rx_state.map_into_stateful(|(_, status)| match status {
            lights::State::Online(PowerColor::On(..)) => lights::PowerState::On,
            lights::State::Online(PowerColor::Off) => lights::PowerState::Off,
            lights::State::Offline => lights::PowerState::Offline,
        });
        spawn(async move {
            let mut rx = rx.subscribe().await;
            while let Ok((_, status)) = rx.recv().await {
                send_power_state(&mqtt, &status, &topic_substr);
            }
        });
    }

    {
        let psr = state.persistent_state_database.for_name(&topic_substr);
        let scene: String = psr.load().unwrap_or_default();
        let scene = Entities::Scenes::from_str(&scene).unwrap_or_default();

        let mqtt = state.mqtt.clone();
        let topic_substr: String = topic_substr;
        spawn(async move {
            let mut state = {
                let scene = scene;
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
            state.tx.try_send(PowerColor::Off);

            loop {
                tokio::select! {
                    Ok(command) = rx_command_s.recv() => {
                        debug!("Got command: {:?}", command);
                        match command {
                            Command::Light2(command) => {
                                process_command(&mut state, command).await;
                            }
                            _ => {
                                error!("Invalid command, expected light2, got {:?}", command);
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

async fn process_command<Entity>(state: &mut LightState<Entity>, command: Light2Command)
where
    Entity: GetSceneEntity + Send + Sync + 'static,
    Entity::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entity::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    match command {
        Light2Command::TurnOn { scene } => {
            if let Ok(scene) = Entity::Scenes::from_str(&scene) {
                set_scene(state, scene).await;
            } else {
                error!("Invalid scene: {}", scene);
            }
        }
        Light2Command::TurnOff => {
            let scene = Entity::Scenes::default();
            set_scene(state, scene).await;
        }

        Light2Command::Flash => {
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

fn send_state(mqtt: &Mqtt, state: &lights::State, topic_substr: &str) {
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

fn send_power_state(mqtt: &Mqtt, power_state: &PowerState, topic_substr: &str) {
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

fn send_scene<Scene: ScenesTrait>(mqtt: &Mqtt, scene: &Scene, topic_substr: &str) {
    let topic = format!("state/{topic_substr}/scene");
    let msg = MqttMessage::new(topic, scene.to_string(), true, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}

struct PassageEntities {
    all: Receiver<PowerColor>,
    cupboard: Receiver<PowerColor>,
    bathroom: Receiver<PowerColor>,
    bedroom: Receiver<PowerColor>,
}

fn run_passage_multiplexer(
    entities: PassageEntities,
    name: impl Into<String>,
) -> Receiver<PowerColor> {
    let (tx, rx) = create_stateless_entity(name);
    spawn(async move {
        let mut all = entities.all.subscribe().await;
        let mut cupboard = entities.cupboard.subscribe().await;
        let mut bathroom = entities.bathroom.subscribe().await;
        let mut bedroom = entities.bedroom.subscribe().await;

        let mut all_colors = PowerColor::Off;
        let mut cupboard_colors = PowerColor::Off;
        let mut bathroom_colors = PowerColor::Off;
        let mut bedroom_colors = PowerColor::Off;

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
                    kelvin: 0,
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

    rx
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
