#[derive(Clone)]
pub struct Id {
    pub location: String,
    pub device: String,
}

impl Id {
    pub fn new(location: &str, device: &str) -> Self {
        Self {
            location: location.to_string(),
            device: device.to_string(),
        }
    }
    pub fn get_name(&self, postfix: &str) -> String {
        format!("{}_{}_{postfix}", self.location, self.device)
    }
    pub fn get_state_topic(&self, component: &str) -> String {
        format!("state/{}/{}/{component}", self.location, self.device)
    }
    pub fn get_command_topic(&self, params: &[&str]) -> String {
        match params.join("/").as_str() {
            "" => format!("command/{}/{}", self.location, self.device),
            params => format!("command/{}/{}/{params}", self.location, self.device),
        }
    }
}
