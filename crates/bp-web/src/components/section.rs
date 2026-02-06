use leptos::prelude::*;

/// Section component - wrapper with decorative ASCII border and anchor link
#[component]
pub fn Section(#[prop(into)] id: String, #[prop(into)] title: String, children: Children) -> impl IntoView {
    let anchor_href = format!("#{}", id);

    view! {
        <section id=id class="mb-8">
            <h2 class="font-bold uppercase mb-3">
                {format!("\u{2500}\u{2524} {} \u{251C}\u{2500}", title)}
                <a href=anchor_href class="section-anchor ml-1">" \u{00A7}"</a>
            </h2>
            <div class="pl-4 border-l border-dashed border-[var(--rule)]">
                {children()}
            </div>
        </section>
    }
}
