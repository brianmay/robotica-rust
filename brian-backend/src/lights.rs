use std::{str::FromStr, time::Duration};

use log::{debug, error};
use robotica_backend::{
    devices::lifx::{run_device, Device},
    entities::{self, Receiver, Sender, Subscription},
    services::{
        mqtt::Mqtt,
        persistent_state::{self, PersistentStateRow},
    },
    spawn,
};
use robotica_common::{
    mqtt::{MqttMessage, QoS},
    robotica::{
        commands::{Command, Light2Command},
        lights::{self, Colors, PowerColor, HSBK},
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
            Self::Off => "off",
        }
        .to_string()
    }
}

impl ScenesTrait for StandardScenes {}

struct StandardSceneEntities {
    on: Receiver<PowerColor>,
    auto: Receiver<PowerColor>,
    rainbow: Receiver<PowerColor>,
    off: Receiver<PowerColor>,
}

impl GetSceneEntity for StandardSceneEntities {
    type Scenes = StandardScenes;

    fn get_scene_entity(&self, scene: Self::Scenes) -> Receiver<PowerColor> {
        match scene {
            StandardScenes::On => self.on.clone(),
            StandardScenes::Auto => self.auto.clone(),
            StandardScenes::Rainbow => self.rainbow.clone(),
            StandardScenes::Off => self.off.clone(),
        }
    }
}

fn static_entity(pc: PowerColor, name: impl Into<String>) -> Receiver<PowerColor> {
    let (tx, rx) = entities::create_stateless_entity(name);
    tx.try_send(pc);
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
) -> Result<(), persistent_state::Error> {
    let flash_color = PowerColor::On(Colors::Single(HSBK {
        hue: 240.0,
        saturation: 50.0,
        brightness: 100.0,
        kelvin: 3500,
    }));

    let on_color = Colors::Single(HSBK {
        hue: 0.0,
        saturation: 0.0,
        brightness: 100.0,
        kelvin: 3500,
    });

    let entities = StandardSceneEntities {
        on: static_entity(PowerColor::On(on_color), "On"),
        auto: mqtt_entity(state, topic_substr, "auto"),
        rainbow: rainbow_entity("rainbow"),
        off: static_entity(PowerColor::Off, "off"),
    };

    let (tx, rx) = run_device(id, discover);

    {
        let mqtt = state.mqtt.clone();
        let topic_substr = topic_substr.to_string();
        spawn(async move {
            let mut rx = rx.subscribe().await;
            while let Ok((_, status)) = rx.recv().await {
                send_state(&mqtt, &status, &topic_substr);
            }
        });
    }

    run_light(state, tx, entities, topic_substr, flash_color)
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

fn run_light<Entities>(
    state: &mut crate::State,
    tx: Sender<PowerColor>,
    entities: Entities,
    topic_substr: impl Into<String>,
    flash_color: PowerColor,
) -> Result<(), persistent_state::Error>
where
    Entities: GetSceneEntity + Send + Sync + 'static,
    Entities::Scenes: ScenesTrait + Copy + Send + Sync + 'static,
    <Entities::Scenes as FromStr>::Err: Send + Sync + 'static,
{
    let topic_substr: String = topic_substr.into();
    let topic = &format!("command/{topic_substr}");
    let rx_command = state
        .subscriptions
        .subscribe_into_stateless::<Command>(topic);

    {
        let psr = state.persistent_state_database.for_name(&topic_substr)?;
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

    Ok(())
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

fn send_scene<Scene: ScenesTrait>(mqtt: &Mqtt, scene: &Scene, topic_substr: &str) {
    let topic = format!("state/{topic_substr}/scene");
    let msg = MqttMessage::new(topic, scene.to_string(), true, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}
