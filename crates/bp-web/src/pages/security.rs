use crate::config::CONFIG;
use leptos::prelude::*;
use leptos_meta::{Link, Meta, Title};

use crate::components::Section;

#[component]
pub fn SecurityPage() -> impl IntoView {
    let twitter_url = format!("https://x.com/{}", CONFIG.contact.twitter);
    let twitter_url2 = twitter_url.clone();
    let twitter_url3 = twitter_url.clone();
    let canonical = format!("{}/security", CONFIG.base_url);

    view! {
        <Title text="Security Policy - Block Parliament Validator" />
        <Meta name="description" content="Block Parliament validator security policy. Key management, infrastructure hardening, access control, monitoring, and incident response procedures." />
        <Link rel="canonical" href=canonical />
        <main class="max-w-[80ch] mx-auto px-4 py-4 md:py-8">
            // Header - responsive, matches Section-style pattern
            <header class="mb-8 text-center">
                <h1 class="text-xl font-bold mb-2">
                    "\u{2500}\u{2524} " {CONFIG.name} " Security Policy \u{251C}\u{2500}"
                </h1>
                <div class="text-[var(--ink-light)]">
                    "Last updated: January 2026"
                </div>
                <div class="mt-2">
                    <a href="/" class="text-sm">"\u{2190} back to home"</a>
                </div>
            </header>

            // Overview
            <Section id="overview" title="Overview">
                <p class="mb-3">
                    "Block Parliament is a Solana mainnet validator operated by "
                    <strong>"Gabe Rodriguez"</strong>
                    ", a core contributor to the Agave validator client at Anza. This document describes the security measures and operational practices in place to protect delegator stake and maintain reliable validator operations."
                </p>
                <p>
                    <strong>"Important:"</strong>
                    " When you delegate, your SOL moves to a stake account that remains under your control. Validators cannot access, move, or withdraw your delegated stake."
                </p>
            </Section>

            // Key Management
            <Section id="keys" title="Key Management">
                <div class="space-y-3">
                    <div>
                        <strong>"Withdrawal Authority Separation"</strong>
                        <p class="mt-1">
                            "The validator's withdrawal authority key ("
                            <code class="text-sm bg-[var(--rule)] px-1">{CONFIG.withdraw_authority}</code>
                            ") is stored separately from the validator identity and vote account keys. This key is kept offline and never resides on the validator server, preventing unauthorized fund access even in the event of server compromise."
                        </p>
                    </div>
                    <div>
                        <strong>"Identity Key Protection"</strong>
                        <p class="mt-1">
                            "The validator identity key is stored on the server with restricted file permissions (owned by the "
                            <code>"sol"</code>
                            " user, mode 600). Administrative access (via the "
                            <code>"ubuntu"</code>
                            " account) is separate from the validator process account ("
                            <code>"sol"</code>
                            "), which has no sudo privileges."
                        </p>
                    </div>
                    <div>
                        <strong>"Hardware Wallet Backup"</strong>
                        <p class="mt-1">
                            "Critical keys are backed up to hardware wallets stored in secure physical locations. Seed phrases are never stored digitally or transmitted over networks."
                        </p>
                    </div>
                </div>
            </Section>

            // Infrastructure
            <Section id="infrastructure" title="Infrastructure">
                <div class="space-y-3">
                    <div>
                        <strong>"Dedicated Bare-Metal Server"</strong>
                        <p class="mt-1">
                            "The validator runs on dedicated bare-metal hardware (not shared cloud VMs) hosted in a professional data center with redundant power and network connectivity. Hardware specs: AMD EPYC 24-core CPU, 377 GB RAM, NVMe storage in RAID configuration."
                        </p>
                    </div>
                    <div>
                        <strong>"Hardened Operating System"</strong>
                        <p class="mt-1">
                            "Linux installation with only essential packages plus monitoring agents (Alloy for metrics, Prometheus exporters). The validator process runs under a dedicated unprivileged user account."
                        </p>
                    </div>
                    <div>
                        <strong>"Network Security"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"\u{2022} Strict firewall rules: only necessary ports exposed (Solana gossip/turbine/repair, SSH, metrics exporter)"</li>
                            <li>"\u{2022} SSH access via public-key authentication only (password auth disabled)"</li>
                            <li>"\u{2022} fail2ban active with aggressive settings (5 attempts \u{2192} 12hr ban)"</li>
                            <li>"\u{2022} DDoS mitigation provided at the data center level"</li>
                        </ul>
                    </div>
                </div>
            </Section>

            // Access Control
            <Section id="access" title="Access Control">
                <div class="space-y-3">
                    <div>
                        <strong>"Limited Personnel"</strong>
                        <p class="mt-1">
                            "Server access is limited to the operator (Gabe Rodriguez) and one contractor (Christopher Vannelli). No other third-party vendors have access to validator infrastructure."
                        </p>
                    </div>
                    <div>
                        <strong>"User Isolation"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"\u{2022} Administrator account (" <code>"ubuntu"</code> ") separate from validator process account (" <code>"sol"</code> ")"</li>
                            <li>"\u{2022} SSH root login disabled"</li>
                            <li>"\u{2022} Validator user cannot sudo or access admin functions"</li>
                        </ul>
                    </div>
                </div>
            </Section>

            // Monitoring
            <Section id="monitoring" title="Monitoring & Alerting">
                <div class="space-y-3">
                    <div>
                        <strong>"24/7 Automated Monitoring"</strong>
                        <p class="mt-1">
                            "A dedicated watchtower service monitors validator health from an independent location (separate from the validator itself). Alerts are sent via Telegram for:"
                        </p>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"\u{2022} Validator delinquency (not voting)"</li>
                            <li>"\u{2022} Health check failures"</li>
                            <li>"\u{2022} Vote account issues"</li>
                        </ul>
                    </div>
                    <div>
                        <strong>"Metrics Collection"</strong>
                        <p class="mt-1">
                            "System and validator metrics (CPU, memory, disk, slot lag, vote performance) are collected and stored in Grafana Cloud for trend analysis and incident investigation."
                        </p>
                    </div>
                    <div>
                        <strong>"Public Performance Data"</strong>
                        <p class="mt-1">
                            "Validator performance is publicly verifiable via "
                            <a href=CONFIG.links.stakewiz target="_blank" rel="noopener noreferrer">"Stakewiz"</a>
                            ", "
                            <a href=CONFIG.links.validators_app target="_blank" rel="noopener noreferrer">"validators.app"</a>
                            ", and on-chain data."
                        </p>
                    </div>
                </div>
            </Section>

            // Software Updates
            <Section id="updates" title="Software & Updates">
                <div class="space-y-3">
                    <div>
                        <strong>"Validator Client"</strong>
                        <p class="mt-1">
                            "Running the Jito-enhanced Agave client for MEV rewards. As an Anza core developer, the operator has deep familiarity with the client codebase and can respond quickly to issues."
                        </p>
                    </div>
                    <div>
                        <strong>"Update Process"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"\u{2022} New releases tracked via Solana Tech Discord"</li>
                            <li>"\u{2022} Updates tested on testnet validator before mainnet"</li>
                            <li>"\u{2022} Tower file backed up before any upgrade (prevents consensus issues)"</li>
                            <li>"\u{2022} OS security patches applied regularly"</li>
                        </ul>
                    </div>
                    <div>
                        <strong>"No Unproven Modifications"</strong>
                        <p class="mt-1">
                            "The validator runs standard Jito-Agave releases without custom consensus modifications that could affect network behavior."
                        </p>
                    </div>
                </div>
            </Section>

            // MEV
            <Section id="mev" title="MEV & Jito Integration">
                <div class="space-y-3">
                    <p>
                        "Block Parliament runs Jito MEV infrastructure. MEV tips are distributed automatically by Jito's on-chain programs\u{2014}the validator receives its configured commission, and Jito distributes the remainder to stakers."
                    </p>
                    <div>
                        <strong>"Configuration"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"\u{2022} Block Engine: Frankfurt (eu-frankfurt)"</li>
                            <li>"\u{2022} Tip programs: Official Jito mainnet contracts"</li>
                            <li>
                                "\u{2022} Current commission rates: "
                                <a href=CONFIG.links.solscan target="_blank" rel="noopener noreferrer">"view on Solscan \u{2197}"</a>
                            </li>
                        </ul>
                    </div>
                </div>
            </Section>

            // Incident Response
            <Section id="incidents" title="Incident Response">
                <div class="space-y-3">
                    <p>
                        "In the event of a security incident or validator issue, the operator follows these procedures:"
                    </p>
                    <ul class="list-none space-y-1">
                        <li><strong>"1. Detection"</strong> " \u{2014} Automated alerts or manual observation"</li>
                        <li><strong>"2. Assessment"</strong> " \u{2014} Determine scope and severity"</li>
                        <li><strong>"3. Containment"</strong> " \u{2014} Isolate affected systems if needed"</li>
                        <li><strong>"4. Resolution"</strong> " \u{2014} Apply fixes, restore service"</li>
                        <li><strong>"5. Review"</strong> " \u{2014} Document lessons learned, improve processes"</li>
                    </ul>
                    <p class="mt-3">
                        "For issues affecting delegators, updates will be posted via "
                        <a href=twitter_url2.clone() target="_blank" rel="noopener noreferrer">
                            "@" {CONFIG.contact.twitter}
                        </a>
                        " on X."
                    </p>
                </div>
            </Section>

            // Verify
            <Section id="verify" title="Verify On-Chain">
                <p>"All claims on this page can be verified independently:"</p>
                <ul class="mt-2 list-none space-y-1">
                    <li>
                        "\u{2022} "
                        <a href=CONFIG.links.solscan target="_blank" rel="noopener noreferrer">"Vote account on Solscan \u{2197}"</a>
                        " \u{2014} commission, authority keys"
                    </li>
                    <li>
                        "\u{2022} "
                        <a href=CONFIG.links.stakewiz target="_blank" rel="noopener noreferrer">"Performance on Stakewiz \u{2197}"</a>
                        " \u{2014} uptime, skip rate, APY"
                    </li>
                    <li>
                        "\u{2022} Withdraw authority: "
                        <code class="text-sm bg-[var(--rule)] px-1">{CONFIG.withdraw_authority}</code>
                    </li>
                </ul>
            </Section>

            // Contact
            <Section id="contact" title="Security Contact">
                <p>
                    "To report security concerns or vulnerabilities related to Block Parliament validator operations:"
                </p>
                <ul class="mt-2 list-none space-y-1">
                    <li>
                        <strong>"X/Twitter:"</strong> " "
                        <a href=twitter_url3 target="_blank" rel="noopener noreferrer">
                            "@" {CONFIG.contact.twitter}
                        </a>
                    </li>
                    <li>
                        <strong>"Telegram:"</strong> " @grod220"
                    </li>
                </ul>
            </Section>

            // Footer
            <footer class="mt-8 pt-4 border-t border-dashed border-[var(--rule)] text-center text-[var(--ink-light)] text-sm">
                <a href="/">"\u{2190} back to home"</a>
            </footer>
        </main>
    }
}
