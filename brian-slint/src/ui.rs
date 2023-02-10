//! Code for UI

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

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
use ::slint::{ComponentHandle, Model, ModelRc, RgbaColor, VecModel};
use futures::{stream::FuturesUnordered, Future, StreamExt};

use robotica_backend::entities::{self, RecvError};
use robotica_common::{
    controllers::{lights2, switch, Action, ConfigTrait, ControllerTrait, DisplayState, Label},
    mqtt::{MqttMessage, QoS},
};
use tokio::{select, sync::mpsc};
use tracing::error;

#[allow(dead_code)]
enum ButtonConfig {
    Light2Config(lights2::Config),
    DeviceConfig(switch::Config),
}

#[allow(dead_code)]
enum Icon {
    Light,
    Fan,
}

#[allow(dead_code)]
struct LabeledButtonConfig {
    bc: ButtonConfig,
    title: String,
    icon: Icon,
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
pub fn run_gui(state: &Arc<RunningState>) {
    let lbc_list = vec![
        Arc::new(LabeledButtonConfig {
            bc: ButtonConfig::Light2Config(lights2::Config {
                topic_substr: format!("{}/Light", state.location),
                action: Action::Toggle,
                scene: "on".into(),
            }),
            title: "On".into(),
            icon: Icon::Light,
        }),
        Arc::new(LabeledButtonConfig {
            bc: ButtonConfig::Light2Config(lights2::Config {
                topic_substr: format!("{}/Light", state.location),
                action: Action::Toggle,
                scene: "auto".into(),
            }),
            title: "Auto".into(),
            icon: Icon::Light,
        }),
        Arc::new(LabeledButtonConfig {
            bc: ButtonConfig::Light2Config(lights2::Config {
                topic_substr: format!("{}/Light", state.location),
                action: Action::Toggle,
                scene: "rainbow".into(),
            }),
            title: "Rainbow".into(),
            icon: Icon::Light,
        }),
    ];

    let (tx_click, rx_click) = {
        let mut rx_click = Vec::with_capacity(lbc_list.len());
        let mut tx_click = Vec::with_capacity(lbc_list.len());
        for _ in 0..lbc_list.len() {
            let (tx, rx) = mpsc::channel::<()>(1);
            rx_click.push(rx);
            tx_click.push(tx);
        }
        (tx_click, rx_click)
    };

    let ui = slint::AppWindow::new();
    ui.hide();

    let icons = ui.get_all_icons();

    let all_buttons: Vec<slint::RoboticaButtonData> = lbc_list
        .iter()
        .map(|lbc| {
            let display_state = DisplayState::Unknown;
            get_button_data(lbc, display_state, &icons)
        })
        .collect();
    ui.set_buttons(ModelRc::new(VecModel::from(all_buttons)));

    ui.on_clicked_button(move |button| {
        let button = usize::try_from(button).unwrap_or(0);
        tx_click
            .get(button)
            .unwrap()
            .try_send(())
            .unwrap_or_else(|_| {
                error!("Failed to send click event");
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

    for (i, (lbc, rx_click)) in lbc_list.into_iter().zip(rx_click).enumerate() {
        let state = state.clone();
        let handle_weak = ui.as_weak();
        let mut rx_click = rx_click;

        tokio::spawn(async move {
            let lbc = lbc;

            let mut controller: Box<dyn ControllerTrait + Send + Sync> = match &lbc.bc {
                ButtonConfig::Light2Config(config) => Box::new(config.create_controller()),
                ButtonConfig::DeviceConfig(config) => Box::new(config.create_controller()),
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
                            println!("Sending command: {c:?}");
                            let message = MqttMessage::new(c.topic.clone(), c.payload.clone(), false, QoS::AtLeastOnce);
                            state.mqtt.try_send(message);
                        });
                    }

                    Ok((label, msg)) = select_ok(f) => {
                        println!("Received message: {i} {label:?} {msg:?}");
                        controller.process_message(label, msg.payload);

                        let display_state = controller.get_display_state();
                        let lbc = lbc.clone();
                        handle_weak
                            .upgrade_in_event_loop(move |handle| {
                                let icons = handle.get_all_icons();
                                let button = get_button_data(&lbc, display_state, &icons);
                                // let all_buttons = vec![button];

                                let buttons = handle.get_buttons();
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

    ui.run();
}

fn get_button_data(
    lbc: &LabeledButtonConfig,
    display_state: DisplayState,
    images: &slint::AllIcons,
) -> slint::RoboticaButtonData {
    #[allow(clippy::redundant_clone)]
    let image = get_image(lbc, display_state, images).clone();
    let state = get_state_text(display_state).into();
    let color = get_color(display_state).into();
    let text_color = get_text_color(display_state).into();

    slint::RoboticaButtonData {
        image,
        title: lbc.title.clone().into(),
        state,
        color,
        text_color,
    }
}

const fn get_color(display_state: DisplayState) -> RgbaColor<u8> {
    match display_state {
        DisplayState::HardOff => RgbaColor {
            red: 255u8,
            green: 255u8,
            blue: 255u8,
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
            green: 0u8,
            blue: 0u8,
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
    lbc: &LabeledButtonConfig,
    display_state: DisplayState,
    images: &'a slint::AllIcons,
) -> &'a ::slint::Image {
    match (&lbc.icon, display_state) {
        (Icon::Light, DisplayState::On) => &images.light.on,
        (Icon::Light, DisplayState::Off | DisplayState::HardOff) => &images.light.off,
        (Icon::Light, DisplayState::AutoOff) => &images.light.auto_off,
        (Icon::Light, DisplayState::Error | DisplayState::Unknown) => &images.light.error,
        (Icon::Fan, DisplayState::On) => &images.fan.on,
        (Icon::Fan, DisplayState::Off | DisplayState::HardOff) => &images.fan.off,
        (Icon::Fan, DisplayState::AutoOff) => &images.fan.auto_off,
        (Icon::Fan, DisplayState::Error | DisplayState::Unknown) => &images.fan.error,
    }
}
