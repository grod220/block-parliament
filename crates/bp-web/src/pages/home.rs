use crate::config::CONFIG;
use leptos::prelude::*;

use crate::components::{AnimatedGradientDashBorder, ExternalLink, Metrics, Section};

#[component]
pub fn HomePage() -> impl IntoView {
    let title = format!("{} \u{1F989}", CONFIG.name); // owl emoji

    view! {
        <main class="max-w-[80ch] mx-auto px-4 py-4 md:py-8">
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
                <div>
                    <strong>"ROUTING"</strong> "   DoubleZero enabled"
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
                    <ExternalLink href=CONFIG.links.validators_app.to_string() label="validators.app".to_string() />
                    <ExternalLink href=CONFIG.links.ibrl.to_string() label="ibrl".to_string() />
                    <ExternalLink href=CONFIG.links.stakewiz.to_string() label="stakewiz".to_string() />
                    <ExternalLink href=CONFIG.links.sfdp.to_string() label="SFDP".to_string() />
                    <ExternalLink href=CONFIG.links.jito.to_string() label="jito stakenet".to_string() />
                    <ExternalLink href=CONFIG.links.solscan.to_string() label="solscan".to_string() />
                </div>
            </Section>

            // Changelog - timeline style
            <Section id="changelog" title="Changelog">
                <div class="pl-3 border-l border-dashed border-[var(--ink-light)]">
                    {CONFIG.changelog.iter().map(|entry| view! {
                        <div class="mb-1">
                            <span class="text-[var(--ink-light)]">{entry.date}</span>
                            "  " {entry.event}
                        </div>
                    }).collect_view()}
                </div>
            </Section>
        </main>
    }
}
