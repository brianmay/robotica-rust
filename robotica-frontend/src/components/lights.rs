use yew::prelude::*;

use crate::services::controllers::{Action, Icon};

use super::button::{Button, LightProps, MusicProps, SwitchProps};
use super::ws_client::WsClient;

#[function_component(Lights)]
pub fn lights_view() -> Html {
    let light_icon = Icon {
        on: "/images/light_on.svg".into(),
        on_other: "/images/light_on_other.svg".into(),
        off: "/images/light_off.svg".into(),
        hard_off: "/images/light_hard_off.svg".into(),
    };

    let wake_up_icon = Icon {
        on: "/images/wake_up_on.svg".into(),
        on_other: "/images/wake_up_on_other.svg".into(),
        off: "/images/wake_up_off.svg".into(),
        hard_off: "/images/wake_up_hard_off.svg".into(),
    };

    let fan_icon = Icon {
        on: "/images/fan_on.svg".into(),
        on_other: "/images/fan_on_other.svg".into(),
        off: "/images/fan_off.svg".into(),
        hard_off: "/images/fan_hard_off.svg".into(),
    };

    let tv_icon = Icon {
        on: "/images/tv_on.svg".into(),
        on_other: "/images/tv_on_other.svg".into(),
        off: "/images/tv_off.svg".into(),
        hard_off: "/images/tv_off.svg".into(),
    };

    html! {
        <WsClient>
            <div>
                <h2>{"Brian's Room"}</h2>
                <Button<LightProps> name={"Brian Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"Brian On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                <Button<SwitchProps> name={"Brian Fan"} topic_substr={"Brian/Fan"} action={Action::Toggle} icon={fan_icon} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon} play_list={"wake_up"} />
            </div>
            <div>
                <h2>{"Passage"}</h2>
                <Button<LightProps> name={"Passage Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"Passage On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
            </div>
            <div>
                <h2>{"Dining Room"}</h2>
                <Button<SwitchProps> name={"TV"} topic_substr={"Dining/TvSwitch"} action={Action::Toggle} icon={tv_icon} />
            </div>

        </WsClient>
    }
}
