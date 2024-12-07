//! Code for UI

#![allow(clippy::unwrap_used)]

use itertools::Itertools;
use std::{sync::Arc, time::Duration};
use tap::Pipe;
use tokio_util::sync::CancellationToken;

mod slint {
    #![allow(clippy::all, clippy::pedantic, clippy::nursery)]
    slint::include_modules!();
}

use crate::{
    partial_command::{self, PartialLine},
    RunningState,
};
use ::slint::{ComponentHandle, Model, ModelRc, RgbaColor, SharedString, VecModel, Weak};
use chrono::{Local, Timelike};
use futures::{stream::FuturesUnordered, Future, StreamExt};
use serde::Deserialize;

use robotica_common::{
    config::{ButtonConfig, ButtonRowConfig, Config as CommonConfig, ControllerConfig, Icon},
    scheduler::{Importance, Status},
};
use robotica_common::{
    controllers::{ConfigTrait, ControllerTrait, DisplayState, Label},
    mqtt::{Json, MqttMessage},
    robotica::audio::Message,
    scheduler::{Sequence, Tags},
};
use robotica_tokio::{
    pipes::{stateful, RecvError, Subscriber, Subscription},
    services::mqtt::MqttTx,
};
use tokio::{
    select,
    sync::mpsc,
    time::{sleep, sleep_until, Instant},
};
use tracing::{debug, error, info};

#[derive(Deserialize)]
pub struct ProgramsConfig {
    turn_screen_on: Vec<String>,
    turn_screen_off: Vec<String>,
}

#[derive(Deserialize)]
pub struct Config {
    number_per_row: u8,
    backlight_on_time: u64,
    name: String,
    programs: ProgramsConfig,
}

#[derive()]
pub struct LoadedProgramsConfig {
    turn_screen_on: PartialLine,
    turn_screen_off: PartialLine,
}

#[derive()]
pub struct LoadedConfig {
    number_per_row: u8,
    backlight_on_time: u64,
    name: String,
    programs: LoadedProgramsConfig,
}

pub struct Button {
    row: usize,
    col: usize,
    config: Arc<ButtonConfig>,
    rx_click: mpsc::Receiver<()>,
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
            backlight_on_time: config.backlight_on_time,
            name: config.name,
            programs,
        })
    }
}

// impl From<Arc<Json<Tags>>> for Vec<slint::TagsForDay> {
fn tags_to_slint(tags: &Json<Tags>) -> ModelRc<slint::TagsForDay> {
    let tags: Vec<slint::TagsForDay> = tags
        .iter()
        .map(|t| {
            let date = t.date.format("%A, %e %B, %Y").to_string();
            let tags: Vec<SharedString> = t.tags.iter().map(SharedString::from).collect();
            let b: VecModel<SharedString> = VecModel::from(tags);
            let c: ModelRc<SharedString> = ModelRc::new(b);
            slint::TagsForDay {
                date: date.into(),
                tags: c,
            }
        })
        .collect();

    ModelRc::new(VecModel::from(tags))
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
    subscription: &mut stateful::Subscription<MqttMessage>,
) -> Result<(Label, MqttMessage), RecvError> {
    let msg = subscription.recv().await?;
    Ok((label, msg))
}

pub enum ScreenCommand {
    TurnOn,
    Message(Message),
}

pub fn run_gui(
    state: RunningState,
    config: Arc<LoadedConfig>,
    rx_screen_command: mpsc::Receiver<ScreenCommand>,
) {
    let state = Arc::new(state);
    let (tx_room, rx_room) = mpsc::channel::<String>(1);

    let ui = slint::AppWindow::new().unwrap();
    ui.set_screen_on(true);
    ui.set_number_per_row(i32::from(config.number_per_row));
    ui.hide().unwrap();

    {
        let handle_weak = ui.as_weak();
        let name = config.name.clone();
        let mqtt = state.mqtt.clone();
        tokio::spawn(async move {
            let topic = format!("robotica/config/{name}");
            let rx = mqtt
                .subscribe_into_stateless::<Json<Arc<CommonConfig>>>(topic)
                .await
                .unwrap();
            let mut rx = rx.subscribe().await;
            let mut rx_room = rx_room;

            #[allow(clippy::collection_is_never_read)]
            let mut _guard = None;
            let mut maybe_common_config: Option<Arc<CommonConfig>> = None;
            let mut maybe_selected_room = None;

            loop {
                select! {
                    msg = rx_room.recv() => if let Some(room) = msg {
                        if let Some(common_config) = &maybe_common_config {
                            maybe_selected_room = common_config.rooms.iter().find(|r| r.title == room).map(|r| r.id.clone());
                        } else {
                            maybe_selected_room = None;
                        }
                    } else {
                        error!("Error receiving room");
                        break;
                    },
                    msg = rx.recv() => match msg {
                        Ok(Json(common_config)) => {
                            maybe_common_config = Some(common_config);
                        }
                        Err(err) => {
                            error!("Error receiving config: {}", err);
                            break;
                        }
                    }
                }

                if let Some(common_config) = maybe_common_config.clone() {
                    let mqtt = mqtt.clone();
                    let cancellation = CancellationToken::new();
                    _guard = cancellation.clone().drop_guard().pipe(Some);

                    let rooms = common_config
                        .rooms
                        .iter()
                        .map(|r| r.title.clone().pipe(SharedString::from))
                        .collect::<Vec<_>>();

                    let selected_room = common_config
                        .rooms
                        .iter()
                        .find(|r| Some(&r.id) == maybe_selected_room.as_ref())
                        .cloned()
                        .or_else(|| common_config.rooms.first().cloned());

                    handle_weak
                        .upgrade_in_event_loop(move |handle| {
                            handle.set_rooms(ModelRc::new(VecModel::from(rooms)));

                            let id = if let Some(selected_room) = selected_room {
                                handle.set_selected_room(selected_room.title.into());
                                Some(selected_room.id)
                            } else {
                                handle.set_selected_room("".into());
                                None
                            };

                            setup_config(
                                &handle,
                                &common_config,
                                id.as_ref(),
                                &mqtt,
                                &cancellation,
                            );
                        })
                        .unwrap();
                }
            }
        });
    }

    monitor_room_change(&ui, tx_room);
    monitor_screen_reset(&state, &ui);
    monitor_display(config, &ui, rx_screen_command);
    monitor_time(&ui);

    ui.run().unwrap();
}

fn setup_config(
    ui: &slint::AppWindow,
    config: &Arc<CommonConfig>,
    room: Option<&String>,
    mqtt: &MqttTx,
    cancellation: &CancellationToken,
) {
    let icons = ui.get_all_icons();
    let mut id = 0;

    let no_room = vec![];
    let buttons: &[ButtonRowConfig] = config
        .rooms
        .iter()
        .find(|r| Some(&r.id) == room)
        .map_or(&no_room, |r| &r.rows);

    let all_buttons: Vec<slint::ButtonRowData> = buttons
        .iter()
        .map(|rc| {
            let buttons: Vec<slint::ButtonData> = rc
                .buttons
                .iter()
                .enumerate()
                .map(|(col, button)| {
                    get_button_data(id + col, button, DisplayState::Unknown, &icons)
                })
                .collect();
            id += buttons.len();

            let b: VecModel<slint::ButtonData> = VecModel::from(buttons);
            let c: ModelRc<slint::ButtonData> = ModelRc::new(b);

            slint::ButtonRowData {
                title: rc.title.clone().into(),
                buttons: c,
            }
        })
        .collect();

    ui.set_buttons(ModelRc::new(VecModel::from(all_buttons)));

    let tx_buttons: Vec<(mpsc::Sender<()>, Button)> = {
        buttons
            .iter()
            .enumerate()
            .flat_map(|(row, rc)| {
                rc.buttons.iter().enumerate().map(move |(col, bd)| {
                    let (tx_click, rx_click) = mpsc::channel::<()>(1);
                    (
                        tx_click,
                        Button {
                            row,
                            col,
                            rx_click,
                            config: Arc::new(bd.clone()),
                        },
                    )
                })
            })
            .collect()
    };

    let (tx_clicks, buttons): (Vec<_>, Vec<_>) = tx_buttons.into_iter().unzip();
    monitor_buttons_presses(ui, tx_clicks);
    monitor_buttons_state(buttons, mqtt, ui, cancellation);

    monitor_tags(config.clone(), mqtt.clone(), ui, cancellation.clone());
    monitor_schedule(config.clone(), mqtt.clone(), ui, cancellation.clone());
}

fn monitor_room_change(ui: &slint::AppWindow, tx_room: mpsc::Sender<String>) {
    ui.on_room_changed(move |room| {
        tx_room.try_send(room.to_string()).unwrap_or_else(|_| {
            error!("Failed to send room change event");
        });
    });
}

fn monitor_screen_reset(state: &Arc<RunningState>, ui: &slint::AppWindow) {
    let tx_screen_command = state.tx_screen_command.clone();
    ui.on_screen_reset(move || {
        tx_screen_command
            .try_send(ScreenCommand::TurnOn)
            .unwrap_or_else(|_| {
                error!("Failed to send screen reset event");
            });
    });
}

fn monitor_buttons_presses(ui: &slint::AppWindow, tx_click: Vec<mpsc::Sender<()>>) {
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
}

fn monitor_buttons_state(
    buttons: Vec<Button>,
    mqtt: &MqttTx,
    ui: &slint::AppWindow,
    cancellation: &CancellationToken,
) {
    for (id, button) in buttons.into_iter().enumerate() {
        let mqtt = mqtt.clone();
        let cancellation = cancellation.clone();
        let handle_weak = ui.as_weak();

        tokio::spawn(async move {
            let button = button;
            let mut rx_click = button.rx_click;
            let lbc = &button.config;

            let mut controller: Box<dyn ControllerTrait + Send + Sync> = match &lbc.controller {
                ControllerConfig::Hdmi(config) => Box::new(config.create_controller()),
                ControllerConfig::Light(config) => Box::new(config.create_controller()),
                ControllerConfig::Switch(config) => Box::new(config.create_controller()),
                ControllerConfig::Zwave(config) => Box::new(config.create_controller()),
                ControllerConfig::Music(config) => Box::new(config.create_controller()),
                ControllerConfig::Tasmota(config) => Box::new(config.create_controller()),
            };

            let requested_subscriptions = controller.get_subscriptions();

            let mut subscriptions = Vec::with_capacity(requested_subscriptions.len());
            for s in controller.get_subscriptions() {
                let label = s.label;
                let s = mqtt.subscribe_into_stateful(s.topic).await.unwrap();
                let s = s.subscribe().await;
                subscriptions.push((label, s));
            }

            loop {
                let f = subscriptions
                    .iter_mut()
                    .map(|(label, s)| receive(*label, s))
                    .map(futures::FutureExt::boxed);

                select! {
                    result = rx_click.recv() => if result == Some(()) {
                        controller.get_press_commands().into_iter().for_each(|message| {
                            mqtt.try_send(message);
                        });
                    } else {
                        debug!("Exiting button press loop");
                        break;
                    },

                    () = cancellation.cancelled() => {
                        debug!("Cancelled; Exiting button press loop");
                        break;
                    },

                    Ok((label, msg)) = select_ok(f) => {
                        controller.process_message(label, msg);

                        let display_state = controller.get_display_state();
                        let lbc = button.config.clone();
                        handle_weak
                            .upgrade_in_event_loop(move |handle| {
                                let icons = handle.get_all_icons();
                                let bd = get_button_data(id, &lbc, display_state, &icons);

                                let row = button.row;
                                let col = button.col;
                                let buttons = handle.get_buttons();
                                if let Some(br) = buttons.row_data(row) {
                                    br.buttons.set_row_data(col, bd);
                                }
                            })
                            .unwrap();
                    }
                }
            }
        });
    }
}

fn monitor_tags(
    config: Arc<CommonConfig>,
    mqtt: MqttTx,
    ui: &slint::AppWindow,
    cancellation: CancellationToken,
) {
    let handle_weak = ui.as_weak();
    tokio::spawn(async move {
        let topic = format!("robotica/{}/tags", config.instance);
        let rx = mqtt
            .subscribe_into_stateless::<Arc<Json<Tags>>>(topic)
            .await
            .unwrap();
        let mut rx = rx.subscribe().await;

        loop {
            select! {
                () = cancellation.cancelled() => {
                    debug!("Cancelled; Exiting tags loop");
                    break;
                }

                msg = rx.recv() => match msg {
                    Ok(msg) => {
                        handle_weak
                        .upgrade_in_event_loop(move |handle| {
                                let tags = tags_to_slint(&msg);
                                handle.set_tags(tags);
                            })
                            .unwrap();
                    }
                    Err(err) => {
                        error!("Error receiving tags: {}", err);
                    }
                }
            }
        }
    });
}

fn get_local_date_for_sequence(sequence: &Sequence) -> chrono::NaiveDate {
    sequence.start_time.with_timezone(&Local).date_naive()
}

fn sequences_to_slint<'a>(
    sequences: impl Iterator<Item = &'a Sequence>,
) -> Vec<slint::SequenceData> {
    sequences
        .map(|s| {
            let tasks: Vec<SharedString> = s.tasks.iter().map(|t| t.title.clone().into()).collect();
            let b: VecModel<SharedString> = VecModel::from(tasks);
            let c: ModelRc<SharedString> = ModelRc::new(b);

            let local = s.start_time.with_timezone(&Local);
            let time = local.format("%H:%M:%S").to_string();
            let status = match s.status {
                Some(Status::Pending) | None => 0,
                Some(Status::InProgress) => 1,
                Some(Status::Completed) => 2,
                Some(Status::Cancelled) => 3,
            };

            slint::SequenceData {
                time: time.into(),
                title: s.title.clone().into(),
                important: matches!(s.importance, Importance::High),
                status,
                tasks: c,
            }
        })
        .collect()
}

fn monitor_schedule(
    config: Arc<CommonConfig>,
    mqtt: MqttTx,
    ui: &slint::AppWindow,
    cancellation: CancellationToken,
) {
    let handle_weak = ui.as_weak();
    tokio::spawn(async move {
        let topic = format!("schedule/{}/pending", config.instance);
        let rx = mqtt
            .subscribe_into_stateless::<Arc<Json<Vec<Sequence>>>>(topic)
            .await
            .unwrap();
        let mut rx = rx.subscribe().await;

        loop {
            select! {
                () = cancellation.cancelled() => {
                    debug!("Cancelled; Exiting schedule loop");
                    break;
                }

                msg = rx.recv() => match msg {
                    Ok(msg) => {
                        handle_weak
                        .upgrade_in_event_loop(move |handle| {
                                let schedule = msg.as_ref();
                                let schedule = schedule
                                    .iter()
                                    .chunk_by(|s| get_local_date_for_sequence(s))
                                    .into_iter()
                                    .map(|(date, sequences)| {
                                        let date = date.format("%A, %e %B, %Y").to_string();
                                        let sequences: Vec<slint::SequenceData> = sequences_to_slint(sequences);
                                        slint::ScheduleData {
                                            date: date.into(),
                                            sequences: ModelRc::new(VecModel::from(sequences)),
                                        }
                                    })
                                    .collect::<Vec<_>>();

                                let b: VecModel<slint::ScheduleData> = VecModel::from(schedule);
                                let c: ModelRc<slint::ScheduleData> = ModelRc::new(b);
                                handle.set_schedule_list(c);
                            })
                            .unwrap();
                    }
                    Err(err) => {
                        error!("Error receiving schedule: {}", err);
                    }
                }
            }
        }
    });
}

fn monitor_time(ui: &slint::AppWindow) {
    let handle_weak = ui.as_weak();
    tokio::spawn(async move {
        loop {
            let time = Local::now();

            #[allow(clippy::cast_possible_wrap)]
            let hour = time.hour() as i32;

            #[allow(clippy::cast_possible_wrap)]
            let minute = time.minute() as i32;

            #[allow(clippy::cast_possible_wrap)]
            let second = time.second() as i32;

            handle_weak
                .upgrade_in_event_loop(move |handle| {
                    handle.set_hour(hour);
                    handle.set_minute(minute);
                    handle.set_second(second);
                })
                .unwrap();

            sleep(Duration::from_secs(1)).await;
        }
    });
}

fn monitor_display(
    config: Arc<LoadedConfig>,
    ui: &slint::AppWindow,
    rx_screen_command: mpsc::Receiver<ScreenCommand>,
) {
    let screen_on_timeout = 30;
    let screen_message_timeout = 15 + config.backlight_on_time;

    let handle_weak = ui.as_weak();
    tokio::spawn(async move {
        let mut state = ScreenState {
            interaction: Some(instant_from_now(screen_on_timeout)),
            message: None,
            backlight: BacklightState::On,
            screen_on: true,
        };
        // Ensure display really is on.
        turn_display_on(&config.programs).await;

        let mut rx_screen_command = rx_screen_command;

        loop {
            select! {
                // We received an external request.
                Some(command) = rx_screen_command.recv() => {
                    match command {
                        ScreenCommand::TurnOn => {
                            state.interaction = Some(instant_from_now(screen_on_timeout));
                            state.message = None;
                        }
                        ScreenCommand::Message(message) => {
                            let title = message.title;
                            let body = message.body;
                            state.message = Some(instant_from_now(screen_message_timeout));
                            handle_weak
                                .upgrade_in_event_loop(|handle| {
                                    handle.set_msg_title(title.into());
                                    handle.set_msg_body(body.into());
                                })
                                .unwrap();
                        }
                    }
                    let handle_weak = handle_weak.clone();
                    state.sync(handle_weak, &config).await;
                }

                // Interaction timer has expired to turn display off.
                Some(()) = interaction_timer_wait(&state) => {
                    state.interaction = None;
                    let handle_weak = handle_weak.clone();
                    state.sync(handle_weak, &config).await;
                }

                // Timer has expired to turn off message.
                Some(()) = message_timer_wait(&state) => {
                    state.message = None;
                    let handle_weak = handle_weak.clone();
                    state.sync(handle_weak, &config).await;
                }

                // Timer has expired to indicate backlight should be on.
                Some(()) = backlight_wait(&state) => {
                    state.screen_on = true;
                    let handle_weak = handle_weak.clone();
                    state.sync(handle_weak, &config).await;
                }
            }
        }
    });
}

fn instant_from_now(secs: u64) -> Instant {
    Instant::now() + Duration::from_secs(secs)
}

#[derive(Clone)]
enum BacklightState {
    On,
    DelayOn(Instant),
    Off,
}

#[derive(Clone)]
struct ScreenState {
    interaction: Option<Instant>,
    message: Option<Instant>,
    backlight: BacklightState,
    screen_on: bool,
}

impl ScreenState {
    const fn should_be_on(&self) -> bool {
        self.interaction.is_some() || self.message.is_some()
    }

    const fn should_be_off(&self) -> bool {
        !self.should_be_on()
    }

    const fn is_on(&self) -> bool {
        matches!(
            self.backlight,
            BacklightState::On | BacklightState::DelayOn(_)
        )
    }

    const fn is_off(&self) -> bool {
        !self.is_on()
    }

    async fn sync(&mut self, handle_weak: Weak<slint::AppWindow>, config: &LoadedConfig) {
        let turn_on_display = self.should_be_on() && !self.is_on();
        let turn_off_display = self.should_be_off() && !self.is_off();

        if turn_on_display {
            self.backlight = BacklightState::DelayOn(instant_from_now(config.backlight_on_time));
            turn_display_on(&config.programs).await;
        } else if turn_off_display {
            self.screen_on = false;
            self.backlight = BacklightState::Off;
            turn_display_off(&config.programs).await;
        }

        let screen_on = self.screen_on;
        let message_on = self.message.is_some();
        handle_weak
            .upgrade_in_event_loop(move |handle| {
                handle.set_display_message(message_on);
                handle.set_screen_on(screen_on);
            })
            .unwrap();
    }
}

async fn interaction_timer_wait(state: &ScreenState) -> Option<()> {
    match state.interaction {
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

async fn backlight_wait(state: &ScreenState) -> Option<()> {
    match state.backlight {
        BacklightState::DelayOn(instant) => {
            sleep_until(instant).await;
            Some(())
        }
        _ => None,
    }
}

fn get_button_data(
    id: usize,
    lbc: &ButtonConfig,
    display_state: DisplayState,
    images: &slint::AllIcons,
) -> slint::ButtonData {
    #[allow(clippy::redundant_clone)]
    let image = get_image(lbc, display_state, images).clone();
    let state = get_state_text(display_state).into();
    let color = get_color(display_state).into();
    let text_color = get_text_color(display_state).into();

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    let id = id as i32;

    slint::ButtonData {
        id,
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
            red: 0x20u8,
            green: 0x20u8,
            blue: 0x20u8,
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

async fn turn_display_off(programs: &LoadedProgramsConfig) {
    info!("Turning off display");
    let cmd = programs.turn_screen_off.to_line();
    info!("Done turning off display");
    if let Err(err) = cmd.run().await {
        error!("Error turning off display: {}", err);
    };
}

async fn turn_display_on(programs: &LoadedProgramsConfig) {
    info!("Turning on display");
    let cmd = programs.turn_screen_on.to_line();
    info!("Done turning on display");
    if let Err(err) = cmd.run().await {
        error!("Error turning on display: {}", err);
    };
}
