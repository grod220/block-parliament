use crate::config::CONFIG;
use leptos::prelude::*;

use crate::components::{AnimatedGradientDashBorder, Metrics, Section};

#[component]
pub fn HomePage() -> impl IntoView {
    let title = format!("{} \u{1F989}", CONFIG.name); // owl emoji

    view! {
        <main class="max-w-[80ch] mx-auto px-4 py-8 md:py-12">
            // Header with animated border
            <header class="mb-8 text-center">
                <AnimatedGradientDashBorder title=title />
                <div class="text-[var(--ink-light)] mt-2">{CONFIG.tagline}</div>
            </header>

            // Addresses - prominent at top
            <div class="mb-6 border border-dashed border-[var(--rule)] p-4">
                <div>
                    <strong>"VOTE"</strong> "     " {CONFIG.vote_account}
                </div>
                <div>
                    <strong>"IDENTITY"</strong> "  " {CONFIG.identity}
                </div>
                <div>
                    <strong>"NETWORK"</strong> "   mainnet-beta"
                </div>
            </div>

            // About
            <Section id="about" title="About">
                <p>
                    "Operated by " <strong>"Gabe Rodriguez"</strong> " ("
                    <a
                        href=format!("https://x.com/{}", CONFIG.contact.twitter)
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        "@" {CONFIG.contact.twitter}
                    </a>
                    "), a core contributor to Solana's Agave validator client and on-chain programs at Anza. A way to experience Solana from the operator's seat, not just the codebase."
                </p>
            </Section>

            // Pages
            <Section id="pages" title="Pages">
                <div class="space-y-1">
                    <div>
                        <a href="/delegate">"delegate \u{2192}"</a>
                    </div>
                    <div>
                        <a href="/security">"security policy \u{2192}"</a>
                    </div>
                </div>
            </Section>

            // Metrics
            <Section id="metrics" title="Metrics">
                <Metrics />
            </Section>

            // Delegate CTA
            <Section id="delegate" title="Delegate">
                <p>
                    "Earn staking rewards while supporting independent infrastructure. "
                    "Fully non-custodial\u{2014}your SOL never leaves your control. "
                    <a href="/delegate">"How to delegate \u{2192}"</a>
                </p>
            </Section>

            // External Links
            <Section id="links" title="External Links">
                <div class="flex flex-wrap gap-2">
                    <a
                        href=CONFIG.links.validators_app
                        target="_blank"
                        rel="noopener noreferrer"
                        class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                    >
                        "validators.app \u{2197}"
                    </a>
                    <a
                        href=CONFIG.links.ibrl
                        target="_blank"
                        rel="noopener noreferrer"
                        class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                    >
                        "ibrl \u{2197}"
                    </a>
                    <a
                        href=CONFIG.links.stakewiz
                        target="_blank"
                        rel="noopener noreferrer"
                        class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                    >
                        "stakewiz \u{2197}"
                    </a>
                    <a
                        href=CONFIG.links.sfdp
                        target="_blank"
                        rel="noopener noreferrer"
                        class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                    >
                        "SFDP \u{2197}"
                    </a>
                    <a
                        href=CONFIG.links.jito
                        target="_blank"
                        rel="noopener noreferrer"
                        class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                    >
                        "jito stakenet \u{2197}"
                    </a>
                    <a
                        href=CONFIG.links.solscan
                        target="_blank"
                        rel="noopener noreferrer"
                        class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                    >
                        "solscan \u{2197}"
                    </a>
                </div>
            </Section>

            // Changelog
            <Section id="changelog" title="Changelog">
                <div>
                    {CONFIG.changelog.iter().map(|entry| view! {
                        <div>
                            <strong>{entry.date}</strong> "  " {entry.event}
                        </div>
                    }).collect_view()}
                </div>
            </Section>
        </main>
    }
}
