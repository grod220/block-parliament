use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::pages::{DelegatePage, HomePage, SecurityPage};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Router>
            <Routes fallback=|| view! { <p>"404 - Page not found"</p> }>
                <Route path=path!("/") view=HomePage />
                <Route path=path!("/delegate") view=DelegatePage />
                <Route path=path!("/security") view=SecurityPage />
            </Routes>
        </Router>
    }
}
