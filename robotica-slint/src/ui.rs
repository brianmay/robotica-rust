//! Code for UI

#![allow(clippy::unwrap_used)]

use std::{sync::Arc, time::Duration};

mod slint {
    #![allow(clippy::wildcard_imports)]
    #![allow(clippy::use_self)]
    #![allow(clippy::used_underscore_binding)]
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::cast_lossless)]
    #![allow(clippy::if_not_else)]
    #![allow(clippy::needless_pass_by_value)]
    #![allow(clippy::too_many_lines)]
    #![allow(clippy::unreadable_literal)]
    #![allow(clippy::semicolon_if_nothing_returned)]
    #![allow(clippy::redundant_else)]
    #![allow(clippy::no_effect_underscore_binding)]
    #![allow(clippy::uninlined_format_args)]
    #![allow(clippy::default_trait_access)]
    #![allow(clippy::redundant_clone)]
    #![allow(clippy::cmp_owned)]
    #![allow(clippy::missing_const_for_fn)]
    #![allow(clippy::match_same_arms)]
    #![allow(clippy::similar_names)]
    #![allow(clippy::items_after_statements)]
    #![allow(clippy::cast_possible_wrap)]

    slint::include_modules!();
}

use crate::{
    partial_command::{self, PartialLine},
    RunningState,
};
use ::slint::{
    ComponentHandle, Image, Model, ModelRc, RgbaColor, SharedPixelBuffer, VecModel, Weak,
};
use futures::{stream::FuturesUnordered, Future, StreamExt};
use serde::Deserialize;

use robotica_backend::entities::{self, RecvError};
use robotica_common::{
    controllers::{
        hdmi, lights2, music2, switch, tasmota, ConfigTrait, ControllerTrait, DisplayState, Label,
    },
    mqtt::MqttMessage,
    robotica::audio::Message,
};
use tokio::{
    select,
    sync::mpsc,
    time::{sleep_until, Instant},
};
use tracing::{error, info};

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ControllerConfig {
    Hdmi(hdmi::Config),
    Light2(lights2::Config),
    Music2(music2::Config),
    Switch(switch::Config),
    Tasmota(tasmota::Config),
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Icon {
    Fan,
    Light,
    Night,
    Schedule,
    Select,
    Speaker,
    Trumpet,
    Tv,
}

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct ButtonConfig {
    controller: ControllerConfig,
    title: String,
    icon: Icon,
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
pub struct TitleConfig {
    title: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum WidgetConfig {
    Button(Arc<ButtonConfig>),
    Title(TitleConfig),
    Nil,
}

#[derive(Deserialize)]
pub struct ProgramsConfig {
    turn_screen_on: Vec<String>,
    turn_screen_off: Vec<String>,
}

#[derive(Deserialize)]
pub struct Config {
    number_per_row: u8,
    buttons: Vec<WidgetConfig>,
    programs: ProgramsConfig,
}

#[derive()]
pub struct LoadedProgramsConfig {
    turn_screen_on: PartialLine,
    turn_screen_off: PartialLine,
}

#[derive()]
pub struct LoadedConfig {
    pub number_per_row: u8,
    pub buttons: Vec<WidgetConfig>,
    pub programs: LoadedProgramsConfig,
}

impl TryFrom<Config> for LoadedConfig {
    type Error = partial_command::Error;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        let programs = LoadedProgramsConfig {
            turn_screen_on: PartialLine::new(config.programs.turn_screen_on)?,
            turn_screen_off: PartialLine::new(config.programs.turn_screen_off)?,
        };
        Ok(Self {
            number_per_row: config.number_per_row,
            buttons: config.buttons,
            programs,
        })
    }
}

async fn select_ok<F, FUTURES, A, B>(futures: FUTURES) -> Result<A, B>
where
    F: Future<Output = Result<A, B>> + Send,
    FUTURES: IntoIterator<Item = F> + Send,
    B: Send,
{
    let mut futures: FuturesUnordered<F> = futures.into_iter().collect();

    let mut last_error: Option<B> = None;
    while let Some(next) = futures.next().await {
        match next {
            Ok(ok) => return Ok(ok),
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    #[allow(clippy::expect_used)]
    Err(last_error.expect("Empty iterator."))
}

async fn receive(
    label: Label,
    subscription: &mut entities::StatelessSubscription<MqttMessage>,
) -> Result<(Label, MqttMessage), RecvError> {
    let msg = subscription.recv().await?;
    Ok((label, msg))
}

pub enum ScreenCommand {
    TurnOn,
    Message(Message),
}

#[allow(clippy::too_many_lines)]
pub fn run_gui(
    state: RunningState,
    config: Arc<LoadedConfig>,
    rx_screen_command: mpsc::Receiver<ScreenCommand>,
) {
    let state = Arc::new(state);

    let (tx_click, rx_click) = {
        let len = config.buttons.len();
        let mut rx_click = Vec::with_capacity(len);
        let mut tx_click = Vec::with_capacity(len);
        for _ in 0..len {
            let (tx, rx) = mpsc::channel::<()>(1);
            rx_click.push(rx);
            tx_click.push(tx);
        }
        (tx_click, rx_click)
    };

    let ui = slint::AppWindow::new().unwrap();
    ui.set_number_per_row(config.number_per_row.into());
    ui.hide().unwrap();

    let icons = ui.get_all_icons();

    let all_widgets: Vec<slint::WidgetData> = config
        .buttons
        .iter()
        .map(|wc| match wc {
            WidgetConfig::Button(bc) => {
                let display_state = DisplayState::Unknown;
                get_button_data(bc, display_state, &icons)
            }
            WidgetConfig::Title(title) => get_title_data(title),
            WidgetConfig::Nil => get_nil_data(),
        })
        .collect();
    ui.set_widgets(ModelRc::new(VecModel::from(all_widgets)));

    ui.on_clicked_widget(move |button| {
        let button = usize::try_from(button).unwrap_or(0);
        tx_click
            .get(button)
            .unwrap()
            .try_send(())
            .unwrap_or_else(|_| {
                error!("Failed to send click event");
            });
    });

    let tx_screen_command = state.tx_screen_command.clone();
    ui.on_screen_reset(move || {
        tx_screen_command
            .try_send(ScreenCommand::TurnOn)
            .unwrap_or_else(|_| {
                error!("Failed to send screen reset event");
            });
    });

    for (i, (lbc, rx_click)) in config.buttons.iter().zip(rx_click).enumerate() {
        if let WidgetConfig::Button(lbc) = lbc {
            let lbc = lbc.clone();
            let state = state.clone();
            let handle_weak = ui.as_weak();
            let mut rx_click = rx_click;

            tokio::spawn(async move {
                let lbc = lbc;

                let mut controller: Box<dyn ControllerTrait + Send + Sync> = match &lbc.controller {
                    ControllerConfig::Hdmi(config) => Box::new(config.create_controller()),
                    ControllerConfig::Light2(config) => Box::new(config.create_controller()),
                    ControllerConfig::Switch(config) => Box::new(config.create_controller()),
                    ControllerConfig::Music2(config) => Box::new(config.create_controller()),
                    ControllerConfig::Tasmota(config) => Box::new(config.create_controller()),
                };

                let requested_subscriptions = controller.get_subscriptions();

                let mut subscriptions = Vec::with_capacity(requested_subscriptions.len());
                for s in controller.get_subscriptions() {
                    let label = s.label;
                    let s = state.mqtt.subscribe(s.topic).await.unwrap();
                    let s = s.subscribe().await;
                    subscriptions.push((label, s));
                }

                loop {
                    let f = subscriptions
                        .iter_mut()
                        .map(|(label, s)| receive(*label, s))
                        .map(futures::FutureExt::boxed);

                    select! {
                        _ = rx_click.recv() => {
                            controller.get_press_commands().into_iter().for_each(|message| {
                                state.mqtt.try_send(message);
                            });
                        }

                        Ok((label, msg)) = select_ok(f) => {
                            controller.process_message(label, msg);

                            let display_state = controller.get_display_state();
                            let lbc = lbc.clone();
                            handle_weak
                                .upgrade_in_event_loop(move |handle| {
                                    let icons = handle.get_all_icons();
                                    let button = get_button_data(&lbc, display_state, &icons);
                                    // let all_buttons = vec![button];

                                    let buttons = handle.get_widgets();
                                    buttons.set_row_data(i, button);
                                    // *dst = button;
                                    // ui.set_buttons
                                })
                                .unwrap();
                        }
                    }
                }
            });
        }
    }

    let handle_weak = ui.as_weak();
    tokio::spawn(async move {
        #[derive(Clone)]
        struct ScreenState {
            on: Option<Instant>,
            message: Option<Instant>,
        }

        impl ScreenState {
            const fn is_on(&self) -> bool {
                self.on.is_some() || self.message.is_some()
            }

            const fn is_off(&self) -> bool {
                !self.is_on()
            }

            async fn sync(
                &mut self,
                prev: Self,
                handle_weak: Weak<slint::AppWindow>,
                config: &LoadedConfig,
            ) {
                if self.message.is_none() && prev.message.is_some() {
                    handle_weak
                        .upgrade_in_event_loop(|handle| {
                            handle.set_display_message(false);
                        })
                        .unwrap();
                }
                if self.is_on() && prev.is_off() {
                    turn_screen_on(handle_weak, &config.programs).await;
                } else if self.is_off() && prev.is_on() {
                    turn_screen_off(handle_weak, &config.programs).await;
                }
            }
        }

        async fn on_timer_wait(state: &ScreenState) -> Option<()> {
            match state.on {
                Some(instant) => {
                    sleep_until(instant).await;
                    Some(())
                }
                None => None,
            }
        }

        async fn message_timer_wait(state: &ScreenState) -> Option<()> {
            match state.message {
                Some(instant) => {
                    sleep_until(instant).await;
                    Some(())
                }
                None => None,
            }
        }

        let mut state = ScreenState {
            on: Some(Instant::now() + Duration::from_secs(30)),
            message: None,
        };
        let mut rx_screen_command = rx_screen_command;

        loop {
            let prev = state.clone();

            select! {
                Some(command) = rx_screen_command.recv() => {
                    match command {
                        ScreenCommand::TurnOn => {
                            state.on = Some(Instant::now() + Duration::from_secs(30));
                            state.message = None;
                        }
                        ScreenCommand::Message(message) => {
                            let (title, body, _priority) = message.into_owned();

                            state.message = Some(Instant::now() + Duration::from_secs(5));
                            handle_weak
                                .upgrade_in_event_loop(|handle| {
                                    handle.set_msg_title(title.into());
                                    handle.set_msg_body(body.into());
                                    handle.set_display_message(true);
                                })
                                .unwrap();
                        }
                    }
                    let handle_weak = handle_weak.clone();
                    state.sync(prev, handle_weak, &config).await;
                }
                Some(_) = on_timer_wait(&state) => {
                    state.on = None;
                    let handle_weak = handle_weak.clone();
                    state.sync(prev, handle_weak, &config).await;
                }
                Some(_) = message_timer_wait(&state) => {
                    state.message = None;
                    let handle_weak = handle_weak.clone();
                    state.sync(prev, handle_weak, &config).await;
                }
            }
        }
    });

    ui.run().unwrap();
}

fn get_button_data(
    lbc: &ButtonConfig,
    display_state: DisplayState,
    images: &slint::AllIcons,
) -> slint::WidgetData {
    #[allow(clippy::redundant_clone)]
    let image = get_image(lbc, display_state, images).clone();
    let state = get_state_text(display_state).into();
    let color = get_color(display_state).into();
    let text_color = get_text_color(display_state).into();

    slint::WidgetData {
        is_button: true,
        is_title: false,
        image,
        title: lbc.title.clone().into(),
        state,
        color,
        text_color,
    }
}

fn get_title_data(lbc: &TitleConfig) -> slint::WidgetData {
    let x = SharedPixelBuffer::new(1, 1);
    let y = Image::from_rgba8(x);

    #[allow(clippy::redundant_clone)]
    slint::WidgetData {
        is_button: false,
        is_title: true,
        image: y,
        title: lbc.title.clone().into(),
        state: "".into(),
        color: RgbaColor {
            red: 30u8,
            green: 30u8,
            blue: 30u8,
            alpha: 255u8,
        }
        .into(),
        text_color: RgbaColor {
            red: 255u8,
            green: 255u8,
            blue: 255u8,
            alpha: 255u8,
        }
        .into(),
    }
}

fn get_nil_data() -> slint::WidgetData {
    let x = SharedPixelBuffer::new(1, 1);
    let y = Image::from_rgba8(x);

    #[allow(clippy::redundant_clone)]
    slint::WidgetData {
        is_button: false,
        is_title: true,
        image: y,
        title: "".into(),
        state: "".into(),
        color: RgbaColor {
            red: 30u8,
            green: 30u8,
            blue: 30u8,
            alpha: 255u8,
        }
        .into(),
        text_color: RgbaColor {
            red: 0u8,
            green: 0u8,
            blue: 0u8,
            alpha: 255u8,
        }
        .into(),
    }
}

const fn get_color(display_state: DisplayState) -> RgbaColor<u8> {
    match display_state {
        DisplayState::HardOff => RgbaColor {
            red: 30u8,
            green: 30u8,
            blue: 30u8,
            alpha: 255u8,
        },
        DisplayState::Error => RgbaColor {
            red: 255u8,
            green: 0u8,
            blue: 0u8,
            alpha: 255u8,
        },
        DisplayState::Unknown => RgbaColor {
            red: 0u8,
            green: 255u8,
            blue: 255u8,
            alpha: 255u8,
        },
        DisplayState::On | DisplayState::AutoOff => RgbaColor {
            red: 0u8,
            green: 255u8,
            blue: 0u8,
            alpha: 255u8,
        },
        DisplayState::Off => RgbaColor {
            red: 0u8,
            green: 00u8,
            blue: 127u8,
            alpha: 255u8,
        },
    }
}

#[allow(clippy::match_same_arms)]
const fn get_text_color(display_state: DisplayState) -> RgbaColor<u8> {
    match display_state {
        DisplayState::HardOff => RgbaColor {
            red: 0u8,
            green: 0u8,
            blue: 0u8,
            alpha: 255u8,
        },
        DisplayState::Error => RgbaColor {
            red: 0u8,
            green: 0u8,
            blue: 0u8,
            alpha: 255u8,
        },
        DisplayState::Unknown => RgbaColor {
            red: 0u8,
            green: 0u8,
            blue: 0u8,
            alpha: 255u8,
        },
        DisplayState::On | DisplayState::AutoOff => RgbaColor {
            red: 0u8,
            green: 0u8,
            blue: 0u8,
            alpha: 255u8,
        },
        DisplayState::Off => RgbaColor {
            red: 255u8,
            green: 255u8,
            blue: 255u8,
            alpha: 255u8,
        },
    }
}

const fn get_state_text(display_state: DisplayState) -> &'static str {
    match display_state {
        DisplayState::HardOff => "Hard Off",
        DisplayState::Error => "Error",
        DisplayState::Unknown => "Unknown",
        DisplayState::On => "On",
        DisplayState::AutoOff => "Auto Off",
        DisplayState::Off => "Off",
    }
}

const fn get_image<'a>(
    lbc: &ButtonConfig,
    display_state: DisplayState,
    images: &'a slint::AllIcons,
) -> &'a ::slint::Image {
    let icon = match &lbc.icon {
        Icon::Fan => &images.fan,
        Icon::Light => &images.light,
        Icon::Night => &images.night,
        Icon::Select => &images.select,
        Icon::Schedule => &images.schedule,
        Icon::Speaker => &images.speaker,
        Icon::Trumpet => &images.trumpet,
        Icon::Tv => &images.tv,
    };

    match display_state {
        DisplayState::On => &icon.on,
        DisplayState::Off | DisplayState::HardOff => &icon.off,
        DisplayState::AutoOff => &icon.auto_off,
        DisplayState::Error | DisplayState::Unknown => &icon.error,
    }
}

async fn turn_screen_off(handle_weak: Weak<slint::AppWindow>, programs: &LoadedProgramsConfig) {
    info!("Turning off display");
    let cmd = programs.turn_screen_off.to_line();
    info!("Done turning off display");
    if let Err(err) = cmd.run().await {
        error!("Error turning off display: {}", err);
    };

    handle_weak
        .upgrade_in_event_loop(|handle| {
            handle.set_screen_off(true);
        })
        .unwrap();
}

async fn turn_screen_on(handle_weak: Weak<slint::AppWindow>, programs: &LoadedProgramsConfig) {
    info!("Turning on display");
    let cmd = programs.turn_screen_on.to_line();
    info!("Done turning on display");
    if let Err(err) = cmd.run().await {
        error!("Error turning on display: {}", err);
    };

    handle_weak
        .upgrade_in_event_loop(|handle| {
            handle.set_screen_off(false);
        })
        .unwrap();
}
