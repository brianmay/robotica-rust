use std::{collections::HashMap, sync::Arc};

use robotica_backend::pipes::stateful;
use robotica_common::datetime::{datetime_to_time_string, time_delta};
use serde::{Deserialize, Serialize};
use serde_tuple::Serialize_tuple;
use thiserror::Error;
use tracing::{error, info};
use url::Url;

use crate::{amber, car};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    url: Url,
    mac: Mac,
}

#[derive(Serialize_tuple, Debug)]
struct Text {
    x: u16,
    y: u16,
    content: String,
    font: String,
    color: u8,
}

#[derive(Serialize_tuple, Debug)]
struct Box {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    color: u8,
}

#[derive(Serialize_tuple, Debug)]
struct Line {
    x1: u16,
    y1: u16,
    x2: u16,
    y2: u16,
    color: u8,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
enum Element {
    Text(Text),
    #[allow(dead_code)]
    Box(Box),
    Line(Line),
}

#[derive(Serialize, Debug)]
struct Template(Vec<Element>);

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Mac(String);

#[derive(Error, Debug)]
enum Error {
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("url error: {0}")]
    Url(#[from] url::ParseError),
}

impl Template {
    async fn send(&self, config: &Config) -> Result<(), Error> {
        let serialized = serde_json::to_string(&self)?;
        let url = config.url.join("jsonupload")?;
        info!("Sending template to {url}: {serialized}");

        let mut params = HashMap::new();
        params.insert("mac", config.mac.0.clone());
        params.insert("json", serialized);

        let response = reqwest::Client::new()
            .post(url)
            .form(&params)
            .send()
            .await?
            .error_for_status()?;

        info!("Got response: {response:?}");
        Ok(())
    }
}

fn pre_inc(y: &mut u16, inc: u16) -> u16 {
    let old_y = *y;
    *y = old_y + inc;
    *y
}

fn post_inc(y: &mut u16, inc: u16) -> u16 {
    let old_y = *y;
    *y = old_y + inc;
    old_y
}

fn header(y: &mut u16, t: String) -> Element {
    Element::Text(Text {
        x: 5,
        y: post_inc(y, 14),
        content: t,
        font: "fonts/bahnschrift20".to_string(),
        color: 2,
    })
}

fn text(y: &mut u16, t: String) -> Element {
    Element::Text(Text {
        x: 5,
        y: pre_inc(y, 14),
        content: t,
        font: "7x14_tf".to_string(),
        color: 1,
    })
}

fn line(y: &mut u16) -> Element {
    *y += 0;
    Element::Line(Line {
        x1: 0,
        y1: *y,
        x2: 152,
        y2: post_inc(y, 0),
        color: 1,
    })
}

pub fn output_amber_car(
    car: &car::Config,
    config: Config,
    state: stateful::Receiver<amber::car::State>,
) {
    let car = Arc::new(car.clone());
    let config = Arc::new(config);
    state.async_for_each(move |(_, state)| {
        let car = car.clone();
        let config = config.clone();
        async move {
            let combined = &state.combined;
            let maybe_user_plan = combined.get_plan();
            let y: &mut u16 = &mut 5;
            let mut template = Template(vec![
                header(y, format!("Car: {}", car.name)),
                line(y),
                text(y, format!("B: {}%", state.battery_level)),
                text(y, format!("MCT: {}%", state.min_charge_tomorrow)),
                text(y, format!("R: {:?}", state.get_result())),
            ]);
            add_plan_to_template(&mut template, y, maybe_user_plan);
            if let Err(e) = template.send(&config).await {
                error!("Failed to send template: {}", e);
            }
        }
    });
}

pub fn output_amber_hotwater(config: Config, state: stateful::Receiver<amber::hot_water::State>) {
    let config = Arc::new(config);
    state.async_for_each(move |(_, state)| {
        let config = config.clone();
        async move {
            let combined = &state.combined;
            let maybe_user_plan = combined.get_plan();
            let y: &mut u16 = &mut 5;
            let mut template = Template(vec![
                header(y, "Hot Water".to_string()),
                line(y),
                // text(y, format!("B: {}%", state.battery_level)),
                // text(y, format!("MCT: {}%", state.min_charge_tomorrow)),
                text(y, format!("R: {:?}", state.get_result())),
            ]);
            add_plan_to_template(&mut template, y, maybe_user_plan);
            if let Err(e) = template.send(&config).await {
                error!("Failed to send template: {}", e);
            }
        }
    });
}

fn add_plan_to_template<R: std::fmt::Debug>(
    template: &mut Template,
    y: &mut u16,
    maybe_user_plan: &amber::MaybeUserPlan<R>,
) {
    if let Some(plan) = &maybe_user_plan.get() {
        template.0.push(text(
            y,
            format!("PS: {}", datetime_to_time_string(plan.get_start_time())),
        ));
        template.0.push(text(
            y,
            format!("PE: {}", datetime_to_time_string(plan.get_end_time())),
        ));
        template
            .0
            .push(text(y, format!("PR: {:?}", plan.get_request())));
        template.0.push(text(
            y,
            format!("PD: {}", time_delta::to_string(plan.get_timedelta())),
        ));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_serialize_template() {
        let template = Template(vec![
            Element::Text(Text {
                x: 0,
                y: 0,
                content: "Hello, World!".to_string(),
                font: "Arial".to_string(),
                color: 0,
            }),
            Element::Box(Box {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
                color: 0,
            }),
            Element::Line(Line {
                x1: 0,
                y1: 0,
                x2: 100,
                y2: 100,
                color: 0,
            }),
        ]);
        let serialized = serde_json::to_string(&template).unwrap();
        assert_eq!(
            serialized,
            r#"[{"text":[0,0,"Hello, World!","Arial",0]},{"box":[0,0,100,100,0]},{"line":[0,0,100,100,0]}]"#
        );
    }
}
