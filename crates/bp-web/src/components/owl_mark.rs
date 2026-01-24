use leptos::prelude::*;

/// Two different pre-generated patterns to mimic independent AnimatedLine components
/// Each pattern is 64 chars (8 segments × 8 chars) with different pseudo-random sequences
/// Pattern A: for left side
const PATTERN_A: &str = "▒ - - - ░ - - - ▒ - - - ▒ - - - ░ - - - ░ - - - ▒ - - - ░ - - - ";
/// Pattern B: for right side (different sequence)
const PATTERN_B: &str = "░ - - - ▒ - - - ░ - - - ░ - - - ▒ - - - ▒ - - - ░ - - - ▒ - - - ";

/// Animated gradient dash border with title - pure CSS ticker animation
/// Left and right sides have different patterns and are desynchronized via animation-delay
#[component]
pub fn AnimatedGradientDashBorder(#[prop(into)] title: String) -> impl IntoView {
    // Repeat patterns for seamless looping (pattern + one extra for shift buffer)
    let pattern_left = PATTERN_A.repeat(2);
    let pattern_right = PATTERN_B.repeat(2);

    view! {
        <div class="select-none whitespace-nowrap flex justify-center items-center">
            <span class="ticker" aria-hidden="true">
                <span class="ticker__track ticker__track--left">{pattern_left}</span>
            </span>
            <span class="font-bold px-4">{title}</span>
            <span class="ticker" aria-hidden="true">
                <span class="ticker__track ticker__track--right">{pattern_right}</span>
            </span>
        </div>
    }
}
