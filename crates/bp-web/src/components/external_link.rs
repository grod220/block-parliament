use leptos::prelude::*;

/// Reusable external link button with consistent styling
#[component]
pub fn ExternalLink(#[prop(into)] href: String, #[prop(into)] label: String) -> impl IntoView {
    view! {
        <a
            href=href
            target="_blank"
            rel="noopener noreferrer"
            class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
        >
            {label} " \u{2197}"
        </a>
    }
}
