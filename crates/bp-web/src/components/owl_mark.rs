use leptos::prelude::*;

const SHADES: &[char] = &['\u{2592}', '\u{2591}']; // ▒ and ░
const SEGMENT: &str = " - - - "; // 3 dashes with spaces

/// Get a random shade character (client-side only)
#[cfg(feature = "hydrate")]
fn get_random_shade() -> char {
    let idx = (js_sys::Math::random() * SHADES.len() as f64) as usize;
    SHADES[idx.min(SHADES.len() - 1)]
}

/// Generate initial line DETERMINISTICALLY for SSR/hydration match
fn generate_static_line(length: usize) -> String {
    let mut line = String::with_capacity(length + 10);
    let mut shade_idx = 0;
    while line.len() < length {
        // Alternate shades deterministically
        line.push(SHADES[shade_idx % SHADES.len()]);
        line.push_str(SEGMENT);
        shade_idx += 1;
    }
    line
}

/// Check if user prefers reduced motion (client-side only)
#[cfg(feature = "hydrate")]
fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok())
        .flatten()
        .map(|mq| mq.matches())
        .unwrap_or(false)
}

/// Animated line component that scrolls ASCII characters
#[component]
fn AnimatedLine() -> impl IntoView {
    // CRITICAL: Use deterministic initial state for both SSR and hydrate
    // This prevents hydration mismatch
    #[cfg(feature = "hydrate")]
    let (line, set_line) = signal(generate_static_line(50));
    #[cfg(not(feature = "hydrate"))]
    let (line, _) = signal(generate_static_line(50));

    // Only run animation on client, and clean up on unmount
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;

        Effect::new(move |_| {
            if prefers_reduced_motion() {
                return;
            }

            let window = web_sys::window().expect("no window");

            let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                set_line.update(|prev| {
                    if prev.len() > 1 {
                        prev.remove(0);
                    }
                    if prev.len() < 50 {
                        prev.push(get_random_shade());
                        prev.push_str(SEGMENT);
                    }
                });
            }) as Box<dyn FnMut()>);

            let interval_id = window
                .set_interval_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(), 400)
                .expect("failed to set interval");

            // Store callback to prevent it from being dropped
            callback.forget();

            // Clean up interval on component unmount
            on_cleanup(move || {
                if let Some(window) = web_sys::window() {
                    window.clear_interval_with_handle(interval_id);
                }
            });
        });
    }

    let display_line = move || {
        let l = line.get();
        l.chars().take(20).collect::<String>()
    };

    view! {
        <span class="text-[var(--ink-light)]">{display_line}</span>
    }
}

/// Animated gradient dash border with title
#[component]
pub fn AnimatedGradientDashBorder(#[prop(into)] title: String) -> impl IntoView {
    view! {
        <div class="select-none overflow-hidden whitespace-nowrap flex justify-center items-center">
            <AnimatedLine />
            <span class="font-bold px-6">{title}</span>
            <AnimatedLine />
        </div>
    }
}
