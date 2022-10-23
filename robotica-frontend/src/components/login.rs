use web_sys::HtmlInputElement;
use yew::functional::*;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::Route;
use crate::User;

#[function_component(Login)]
pub fn login() -> Html {
    let name = use_state(String::new);
    let user = use_context::<User>().expect("No context found.");

    let oninput = {
        let current_name = name.clone();

        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            current_name.set(input.value());
        })
    };

    let onclick = {
        let name = name.clone();
        let user = user;
        Callback::from(move |_| *user.name.borrow_mut() = (*name).clone())
    };

    html! {
        <div class="bg-gray-800 flex w-screen">
            <div class="container mx-auto flex flex-col justify-center items-center">
                <form class="m-4 flex">
                    <input {oninput} class="rounded-l-lg p-4 border-t mr-0 border-b border-l text-gray-800 border-gray-200 bg-white" placeholder="Username"/>
                    <Link<Route> to={Route::Test}> <button {onclick} disabled={name.len()<1} class="px-8 rounded-r-lg bg-violet-600	  text-white font-bold p-4 uppercase border-violet-600 border-t border-b border-r" >{"Go Chatting!"}</button></Link<Route>>
                </form>
            </div>
        </div>
    }
}
