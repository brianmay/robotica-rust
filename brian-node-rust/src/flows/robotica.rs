use serde::Serialize;

#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Power {
    On,
    Off,
    HardOff,
    Error,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaColorOut {
    pub hue: u16,
    pub saturation: u16,
    pub brightness: u16,
    pub kelvin: u16,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaLightCommand {
    pub action: Option<String>,
    pub color: Option<RoboticaColorOut>,
    pub scene: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaDeviceCommand {
    pub action: Option<String>,
}

#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaAutoColor {
    pub power: Power,
    pub color: RoboticaColorOut,
}

pub fn string_to_power(value: String) -> Power {
    match value.as_str() {
        "OFF" => Power::Off,
        "ON" => Power::On,
        "HARD_OFF" => Power::HardOff,
        _ => Power::Error,
    }
}
