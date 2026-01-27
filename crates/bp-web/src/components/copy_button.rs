use leptos::prelude::*;

/// A button that copies text to clipboard with visual feedback
/// Uses inline JavaScript since this is an SSR-only site without hydration
#[component]
pub fn CopyButton(
    /// The text to copy when clicked
    #[prop(into)]
    text: String,
    /// Button label (shown before copy)
    #[prop(into)]
    label: String,
) -> impl IntoView {
    // Inline JS that copies text and shows feedback
    let onclick_js = format!(
        "navigator.clipboard.writeText('{}').then(() => {{ \
            const btn = this; \
            const original = btn.textContent; \
            btn.textContent = 'Copied!'; \
            setTimeout(() => btn.textContent = original, 2000); \
        }})",
        text
    );

    view! {
        <button
            type="button"
            onclick=onclick_js
            class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors cursor-pointer"
        >
            {label}
        </button>
    }
}
