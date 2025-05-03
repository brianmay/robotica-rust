use std::sync::Arc;

use freeswitch_esl::{Esl, EslConnection, EslError};
use robotica_common::{
    mqtt::{MqttMessage, Retain},
    robotica::{
        audio::MessagePriority,
        message::{Audience, Message},
    },
};
use robotica_tokio::{services::mqtt::MqttTx, spawn};
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

use crate::{phone_db, RunningState};

#[derive(Deserialize)]
pub struct Config {
    listen_address: String,
    topic: String,
    audience: Audience,
}

#[derive(Debug)]
struct Call {
    caller_number: String,
    destination_number: String,
}

impl TryFrom<&EslConnection> for Call {
    type Error = ();

    fn try_from(conn: &EslConnection) -> Result<Self, ()> {
        let caller_number = conn.get_info::<String>("Caller-Caller-ID-Number");
        let destination_number = conn.get_info::<String>("variable_original_destination");

        if let (Some(caller_number), Some(destination_number)) = (caller_number, destination_number)
        {
            Ok(Self {
                caller_number,
                destination_number,
            })
        } else {
            Err(())
        }
    }
}

#[derive(Debug)]
enum OurResponse {
    Allow(Call, phone_db::Response),
    #[allow(dead_code)]
    VoiceMail(Call, phone_db::Response),
    Error(Call),
    Unknown,
}

async fn process_our_response(
    conn: EslConnection,
    config: &Config,
    mqtt: MqttTx,
    our_response: OurResponse,
) -> Result<(), EslError> {
    info!("Processing our response {our_response:?}");

    match our_response {
        OurResponse::Allow(call, response) => {
            set_caller_name(&response, &conn)
                .await
                .unwrap_or_else(|err| {
                    // If this failed, we probably are stuffed, but continue anyway.
                    error!("Error setting caller id name: {err}");
                });

            let name = response.name.as_deref().unwrap_or(&call.caller_number);
            let message = format!("Call from {name}");
            send_message(&message, config, &mqtt);
            default_action(&conn).await?;
        }

        OurResponse::VoiceMail(_, response) => {
            set_caller_name(&response, &conn)
                .await
                .unwrap_or_else(|err| {
                    error!("Error setting caller id name: {err}");
                });

            conn.execute("voicemail", "default $${domain} ${dialed_extension}")
                .await?;
            conn.hangup("NORMAL_CLEARING").await?;
        }

        OurResponse::Error(call) => {
            let message = format!("Defaulted call from {}", call.caller_number);
            send_message(&message, config, &mqtt);
            default_action(&conn).await?;
        }

        OurResponse::Unknown => {
            let message = "Call from unknown number";
            send_message(message, config, &mqtt);
            default_action(&conn).await?;
        }
    }

    debug!("Done");
    Ok(())
}

fn send_message(message: &str, config: &Config, mqtt: &MqttTx) {
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
        Retain::NoRetain,
        robotica_common::mqtt::QoS::ExactlyOnce,
    );

    match msg {
        Ok(msg) => {
            debug!("Sending message to mqtt {msg:?}");
            mqtt.try_send(msg);
        }
        Err(err) => error!("Error encoding message: {:?}", err),
    }
}

async fn get_our_response(conn: &EslConnection, phone_db: &phone_db::Config) -> OurResponse {
    let maybe_call = Call::try_from(conn);
    if let Ok(call) = maybe_call {
        let maybe_response =
            get_phone_db(&call.caller_number, &call.destination_number, phone_db).await;

        let action = maybe_response.map(|response| (response.action, response));
        match action {
            Ok((phone_db::Action::Allow, response)) => OurResponse::Allow(call, response),
            Ok((phone_db::Action::VoiceMail, response)) => OurResponse::VoiceMail(call, response),
            Err(err) => {
                error!("Error getting phone_db response: {err}");
                OurResponse::Error(call)
            }
        }
    } else {
        error!("Got call from unknown number to unknown number");
        OurResponse::Unknown
    }
}

async fn process_connection(
    socket: TcpStream,
    config: &Config,
    phone_db: &phone_db::Config,
    mqtt: MqttTx,
) -> Result<(), EslError> {
    debug!("Got connection");
    let conn = Esl::outbound(socket).await?;
    let our_response = get_our_response(&conn, phone_db).await;
    process_our_response(conn, config, mqtt, our_response).await?;
    debug!("Connection closed");
    Ok(())
}

async fn get_phone_db(
    caller_number: &str,
    destination_number: &str,
    phone_db: &phone_db::Config,
) -> Result<phone_db::Response, phone_db::Error> {
    info!("Got call from {caller_number} to {destination_number}");
    let result = phone_db::check_number(caller_number, destination_number, phone_db).await?;
    debug!("phone_db result: {:?}", result);
    Ok(result)
}

async fn set_caller_name(
    result: &phone_db::Response,
    conn: &EslConnection,
) -> Result<(), EslError> {
    if let Some(name) = &result.name {
        debug!("Setting caller id name to {}", name);
        conn.execute("set", &format!("effective_caller_id_name={name}"))
            .await?;
    }
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
                if let Err(err) = process_connection(socket, &config, &phone_db, mqtt).await {
                    error!("Error processing connection: {err}");
                }
                debug!("Connection from {addr} closed");
            });
        }
    });

    Ok(())
}
