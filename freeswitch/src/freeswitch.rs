use std::sync::Arc;

use freeswitch_esl::{Esl, EslConnection, EslError};
use robotica_backend::{services::mqtt::MqttTx, spawn};
use robotica_common::{
    mqtt::MqttMessage,
    robotica::{audio::MessagePriority, message::Message},
};
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

use crate::{phone_db, RunningState};

#[derive(Deserialize)]
pub struct Config {
    listen_address: String,
    topic: String,
    audience: String,
}

async fn process_call(
    conn: EslConnection,
    config: &Config,
    phone_db: &phone_db::Config,
    mqtt: MqttTx,
) -> Result<(), EslError> {
    let caller_number = conn.get_info::<String>("Caller-Caller-ID-Number");
    let destination_number = conn.get_info::<String>("Caller-Destination-Number");

    if let (Some(caller_number), Some(destination_number)) = (caller_number, destination_number) {
        info!("Got call from {caller_number} to {destination_number}");

        let result = phone_db::check_number(&caller_number, &destination_number, phone_db).await;
        debug!("phone_db result: {:?}", result);

        if let Some(name) = &result.name {
            debug!("Setting caller id name to {}", name);
            conn.execute("set", &format!("effective_caller_id_name={name}"))
                .await?;
        }

        match result.action {
            phone_db::Action::Allow => {
                info!("Allowing call");
                let name = result.name.as_deref().unwrap_or(&caller_number);
                debug!("Got name {name}");
                let message = format!("Call from {name}");
                debug!("Sending message: {message}");
                let msg = Message::new(
                    "Home Phone",
                    message,
                    MessagePriority::Low,
                    &config.audience,
                );
                let msg = MqttMessage::from_json(
                    &config.topic,
                    &msg,
                    false,
                    robotica_common::mqtt::QoS::ExactlyOnce,
                );

                match msg {
                    Ok(msg) => {
                        debug!("Sending message to mqtt {msg:?}");
                        mqtt.try_send(msg);
                    }
                    Err(err) => error!("Error encoding message: {:?}", err),
                }

                default_action(&conn).await?;
            }
            phone_db::Action::VoiceMail => {
                info!("Sending to voicemail");
                conn.execute("voicemail", "default $${domain} ${dialed_extension}")
                    .await?;
                conn.hangup("NORMAL_CLEARING").await?;
            }
        }
    } else {
        println!("Got call from unknown number to unknown number");
        default_action(&conn).await?;
    }

    debug!("Done");
    Ok(())
}

async fn default_action(conn: &EslConnection) -> Result<(), EslError> {
    debug!("Bridging connection");
    conn.execute("bridge", "${group_call(home@${domain_name})}")
        .await?;
    conn.execute("answer", "").await?;
    conn.execute("sleep", "1000").await?;
    conn.execute("set", "skip_instructions=true").await?;
    conn.execute("voicemail", "default $${domain} ${dialed_extension}")
        .await?;
    Ok(())
}

pub async fn run(
    running_state: &RunningState,
    config: Config,
    phone_db: phone_db::Config,
) -> Result<(), EslError> {
    let listener = TcpListener::bind(&config.listen_address).await?;
    info!("Listening on {}", config.listen_address);
    let mqtt = running_state.mqtt.clone();

    spawn(async move {
        let config = Arc::new(config);
        let phone_db = Arc::new(phone_db);

        loop {
            let (socket, addr) = match listener.accept().await {
                Ok(s) => s,
                Err(err) => {
                    error!("Error accepting connection: {err}");
                    continue;
                }
            };
            let config = config.clone();
            let phone_db = phone_db.clone();
            let mqtt = mqtt.clone();
            spawn(async move {
                debug!("Got connection from {addr}");
                if let Err(err) = process_connection(socket, config, phone_db, mqtt).await {
                    error!("Error processing connection: {err}");
                }
                debug!("Connection from {addr} closed");
            });
        }
    });

    Ok(())
}

async fn process_connection(
    socket: tokio::net::TcpStream,
    config: Arc<Config>,
    phone_db: Arc<phone_db::Config>,
    mqtt: MqttTx,
) -> Result<(), EslError> {
    let stream = Esl::outbound(socket).await?;
    process_call(stream, &config, &phone_db, mqtt).await?;
    Ok(())
}
