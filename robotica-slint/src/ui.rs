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

use crate::RunningState;
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
    mqtt::{MqttMessage, QoS},
};
use tokio::{
    select,
    sync::mpsc,
    time::{sleep_until, Instant},
};
use tracing::error;

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
#[derive(Deserialize)]
pub struct TitleConfig {
    title: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum WidgetConfig {
    Button(Arc<ButtonConfig>),
    Title(TitleConfig),
    Nil,
}

async fn select_ok<F, FUTURES, A, B>(futs: FUTURES) -> Result<A, B>
where
    F: Future<Output = Result<A, B>> + Send,
    FUTURES: IntoIterator<Item = F> + Send,
    B: Send,
{
    let mut futs: FuturesUnordered<F> = futs.into_iter().collect();

    let mut last_error: Option<B> = None;
    while let Some(next) = futs.next().await {
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
    subscription: &mut entities::Subscription<MqttMessage>,
) -> Result<(Label, MqttMessage), RecvError> {
    let msg = subscription.recv().await?;
    Ok((label, msg))
}

#[allow(clippy::too_many_lines)]
pub fn run_gui(state: RunningState, number_per_row: u8, buttons: &Vec<WidgetConfig>) {
    let state = Arc::new(state);
    let (tx_screen_reset, rx_screen_reset) = mpsc::channel(1);

    let (tx_click, rx_click) = {
        let len = buttons.len();
        let mut rx_click = Vec::with_capacity(len);
        let mut tx_click = Vec::with_capacity(len);
        for _ in 0..len {
            let (tx, rx) = mpsc::channel::<()>(1);
            rx_click.push(rx);
            tx_click.push(tx);
        }
        (tx_click, rx_click)
    };

    let ui = slint::AppWindow::new();
    ui.set_number_per_row(number_per_row.into());
    ui.hide();

    let icons = ui.get_all_icons();

    let all_widgets: Vec<slint::WidgetData> = buttons
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

    ui.on_screen_reset(move || {
        tx_screen_reset.try_send(()).unwrap_or_else(|_| {
            error!("Failed to send screen off event");
        });
    });
    // let ui_handle = ui.as_weak();
    // ui.on_request_increase_value(move || {
    //     let ui = ui_handle.unwrap();
    //     ui.set_counter(ui.get_counter() + 1);
    // });

    // let handle_weak = ui.as_weak();
    // let topic = format!("command/{}/Robotica", state.location);

    // let mqtt = state.mqtt.clone();
    // tokio::spawn(async move {
    //     let rx: Receiver<robotica::commands::Command> =
    //         mqtt.subscribe_into_stateless(topic).await.unwrap();
    //     let mut rx_s = rx.subscribe().await;

    //     while let Ok(command) = rx_s.recv().await {
    //         if let robotica::commands::Command::Audio(command) = command {
    //             let title = command.title;
    //             let message = command.message;

    //             handle_weak
    //                 .upgrade_in_event_loop(move |handle| {
    //                     handle.set_msg_title(format!("{title:?}").into());
    //                     handle.set_msg_text(format!("{message:?}").into());
    //                 })
    //                 .unwrap();
    //         }
    //     }
    // });

    let handle_weak = ui.as_weak();
    tokio::spawn(async move {
        enum ScreenState {
            On(Instant),
            Off,
        }

        async fn timer_wait(state: &ScreenState) -> Option<()> {
            match state {
                ScreenState::On(instant) => {
                    sleep_until(*instant).await;
                    Some(())
                }
                ScreenState::Off => None,
            }
        }

        let mut state = ScreenState::On(Instant::now() + Duration::from_secs(30));
        let mut rx_screen_reset = rx_screen_reset;

        loop {
            select! {
                Some(()) = rx_screen_reset.recv() => {
                    if matches!(state, ScreenState::Off) {
                        turn_screen_on(&handle_weak);
                    }
                    state = ScreenState::On(Instant::now() + Duration::from_secs(30));
                }
                Some(_) = timer_wait(&state) => {
                    turn_screen_off(&handle_weak);
                    state = ScreenState::Off;
                }
            }
        }
    });

    for (i, (lbc, rx_click)) in buttons.iter().zip(rx_click).enumerate() {
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

                let requested_subcriptions = controller.get_subscriptions();

                let mut subscriptions = Vec::with_capacity(requested_subcriptions.len());
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
                            controller.get_press_commands().iter().for_each(|c| {
                                let message = MqttMessage::new(c.topic.clone(), c.payload.clone(), false, QoS::AtLeastOnce);
                                state.mqtt.try_send(message);
                            });
                        }

                        Ok((label, msg)) = select_ok(f) => {
                            controller.process_message(label, msg.payload);

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

    ui.run();
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

fn turn_screen_off(handle_weak: &Weak<slint::AppWindow>) {
    handle_weak
        .upgrade_in_event_loop(|handle| {
            handle.set_screen_off(true);
        })
        .unwrap();
}

fn turn_screen_on(handle_weak: &Weak<slint::AppWindow>) {
    handle_weak
        .upgrade_in_event_loop(|handle| {
            handle.set_screen_off(false);
        })
        .unwrap();
}
