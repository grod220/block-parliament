use crate::config::CONFIG;
use leptos::prelude::*;

use crate::components::{CopyButton, Section};

#[component]
pub fn DelegatePage() -> impl IntoView {
    let twitter_url = format!("https://x.com/{}", CONFIG.contact.twitter);
    let twitter_url2 = twitter_url.clone();

    view! {
        <main class="max-w-[80ch] mx-auto px-4 py-8 md:py-12">
            // Header
            <header class="mb-8">
                <div class="text-center">
                    <h1 class="text-xl font-bold mb-2">
                        "\u{250C}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2510}"
                        <br />
                        "\u{2502} Delegate to Block Parliament        \u{2502}"
                        <br />
                        "\u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2518}"
                    </h1>
                </div>
                <div class="mt-4 text-center">
                    <a href="/" class="text-sm">"\u{2190} back to home"</a>
                </div>
            </header>

            // Quick actions
            <Section id="quick" title="Quick Actions">
                <div class="mb-4">
                    <div class="mb-3 border border-dashed border-[var(--rule)] p-3">
                        <div class="text-[var(--ink-light)] text-sm mb-1">"VOTE ACCOUNT"</div>
                        <code class="break-all">{CONFIG.vote_account}</code>
                    </div>
                    <div class="flex flex-wrap gap-2">
                        <CopyButton text=CONFIG.vote_account.to_string() label="Copy vote account".to_string() />
                        <a
                            href=CONFIG.links.solscan
                            target="_blank"
                            rel="noopener noreferrer"
                            class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                        >
                            "Open in Solscan \u{2197}"
                        </a>
                        <a
                            href=CONFIG.links.stakewiz
                            target="_blank"
                            rel="noopener noreferrer"
                            class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                        >
                            "View on StakeWiz \u{2197}"
                        </a>
                        <a
                            href=CONFIG.links.validators_app
                            target="_blank"
                            rel="noopener noreferrer"
                            class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                        >
                            "validators.app \u{2197}"
                        </a>
                    </div>
                </div>
            </Section>

            // Native Staking Instructions
            <Section id="native" title="Delegate SOL (Native Staking)">
                <p class="mb-4">
                    "Delegation is " <strong>"non-custodial"</strong> ": your SOL moves to a stake account "
                    "that remains under your control. The validator cannot access, move, or withdraw "
                    "your delegated stake."
                </p>

                <div class="space-y-6">
                    // Phantom
                    <div>
                        <h3 class="font-bold mb-2">"Phantom Wallet"</h3>
                        <ol class="list-none space-y-1 pl-4 border-l border-dashed border-[var(--ink-light)]">
                            <li>"1. Open Phantom \u{2192} click \"Stake\" button on home screen"</li>
                            <li>"2. Tap \"Search for a validator\" at the top"</li>
                            <li>"3. Search \"Block Parliament\" or paste the vote address"</li>
                            <li>"4. Select Block Parliament from results"</li>
                            <li>"5. Enter amount you want to stake"</li>
                            <li>"6. Review details \u{2192} tap \"Stake\""</li>
                        </ol>
                    </div>

                    // Solflare
                    <div>
                        <h3 class="font-bold mb-2">"Solflare Wallet"</h3>
                        <ol class="list-none space-y-1 pl-4 border-l border-dashed border-[var(--ink-light)]">
                            <li>"1. Open Solflare \u{2192} go to \"Staking\" tab"</li>
                            <li>"2. Tap \"Stake SOL\" or \"+\" to add new stake"</li>
                            <li>"3. Search \"Block Parliament\" or paste vote address"</li>
                            <li>"4. Enter stake amount"</li>
                            <li>"5. Confirm the transaction"</li>
                        </ol>
                    </div>

                    // Other wallets
                    <div>
                        <h3 class="font-bold mb-2">"Other Wallets"</h3>
                        <p class="pl-4 border-l border-dashed border-[var(--ink-light)]">
                            "Most Solana wallets support staking. Look for a \"Stake\" or \"Earn\" "
                            "section, search for \"Block Parliament\", or paste the vote account address: "
                            <code class="text-sm bg-[var(--rule)] px-1 break-all">{CONFIG.vote_account}</code>
                        </p>
                    </div>
                </div>
            </Section>

            // Liquid Staking
            <Section id="liquid" title="Liquid Stake">
                {move || {
                    if let (Some(symbol), Some(url)) = (CONFIG.lst.symbol, CONFIG.lst.primary_url) {
                        view! {
                            <div>
                                <p class="mb-4">
                                    "Liquid staking lets you stake while keeping your capital liquid. "
                                    "Stake SOL \u{2192} receive " <strong>{symbol}</strong> " tokens that can be used in DeFi."
                                </p>
                                <div class="space-y-3">
                                    <div>
                                        <h3 class="font-bold mb-2">"How it works"</h3>
                                        <ol class="list-none space-y-1 pl-4 border-l border-dashed border-[var(--ink-light)]">
                                            <li>"1. Connect your wallet to the liquid staking app"</li>
                                            <li>"2. Enter the amount of SOL to stake"</li>
                                            <li>"3. Receive " {symbol} " tokens representing your staked SOL"</li>
                                            <li>"4. Use " {symbol} " in DeFi or hold to accumulate rewards"</li>
                                            <li>"5. Unstake anytime by swapping " {symbol} " back to SOL"</li>
                                        </ol>
                                    </div>
                                    <div class="flex flex-wrap gap-2">
                                        <a
                                            href=url
                                            target="_blank"
                                            rel="noopener noreferrer"
                                            class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                                        >
                                            "Liquid stake now \u{2197}"
                                        </a>
                                        {CONFIG.lst.mint_address.map(|mint| {
                                            let explorer_url = format!("https://solscan.io/token/{}", mint);
                                            view! {
                                                <a
                                                    href=explorer_url
                                                    target="_blank"
                                                    rel="noopener noreferrer"
                                                    class="px-3 py-1 border border-dashed border-[var(--rule)] hover:bg-[var(--rule)] transition-colors inline-block"
                                                >
                                                    "View token on Solscan \u{2197}"
                                                </a>
                                            }
                                        })}
                                    </div>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div>
                                <p class="mb-4 text-[var(--ink-light)]">
                                    "Single-validator liquid staking token is in development."
                                </p>
                                <p>
                                    "Follow "
                                    <a
                                        href=twitter_url.clone()
                                        target="_blank"
                                        rel="noopener noreferrer"
                                    >
                                        "@" {CONFIG.contact.twitter}
                                    </a>
                                    " for updates on when liquid staking becomes available."
                                </p>
                            </div>
                        }.into_any()
                    }
                }}
            </Section>

            // FAQ
            <Section id="faq" title="FAQ">
                <div class="space-y-4">
                    <div>
                        <h3 class="font-bold">"Is staking custodial?"</h3>
                        <p class="mt-1 pl-4 border-l border-dashed border-[var(--ink-light)]">
                            <strong>"No."</strong> " When you delegate, your SOL moves to a stake account "
                            "that you control. The validator cannot access your funds. You can undelegate "
                            "at any time without the validator's permission."
                        </p>
                    </div>

                    <div>
                        <h3 class="font-bold">"Can the validator move my SOL?"</h3>
                        <p class="mt-1 pl-4 border-l border-dashed border-[var(--ink-light)]">
                            <strong>"No."</strong> " Validators only have authority to use your stake for voting. "
                            "They cannot withdraw, transfer, or access your SOL in any way. The withdrawal "
                            "authority (the key that can move funds) remains with you."
                        </p>
                    </div>

                    <div>
                        <h3 class="font-bold">"How long to activate/deactivate stake?"</h3>
                        <p class="mt-1 pl-4 border-l border-dashed border-[var(--ink-light)]">
                            "Solana uses a warmup/cooldown period. Stake activates at the start of the "
                            "next epoch (epochs are ~2-3 days). Deactivation also takes until the end of "
                            "the current epoch. During cooldown, your stake doesn't earn rewards but "
                            "remains in your control."
                        </p>
                    </div>

                    <div>
                        <h3 class="font-bold">"What are the risks?"</h3>
                        <div class="mt-1 pl-4 border-l border-dashed border-[var(--ink-light)]">
                            <ul class="list-none space-y-1">
                                <li>
                                    "\u{2022} " <strong>"Performance risk:"</strong> " If the validator performs poorly "
                                    "(high skip rate, downtime), you may earn lower rewards than other validators."
                                </li>
                                <li>
                                    "\u{2022} " <strong>"Slashing risk:"</strong> " Solana does not currently implement "
                                    "slashing, but this may change in the future."
                                </li>
                                <li>
                                    "\u{2022} " <strong>"Smart contract risk (LST only):"</strong> " Liquid staking "
                                    "involves smart contracts that could have bugs. Native staking has no smart "
                                    "contract risk."
                                </li>
                            </ul>
                        </div>
                    </div>

                </div>
            </Section>

            // Security Warning
            <Section id="security" title="Security Notice">
                <div class="border border-dashed border-[var(--rule)] p-3 mb-4">
                    <p class="font-bold mb-2">"\u{26A0} We will never:"</p>
                    <ul class="list-none space-y-1">
                        <li>"\u{2022} DM you asking for seed phrases or private keys"</li>
                        <li>"\u{2022} Ask you to connect your wallet to unknown sites"</li>
                        <li>"\u{2022} Request you send SOL to receive rewards"</li>
                        <li>"\u{2022} Ask for remote access to your device"</li>
                    </ul>
                </div>
                <p>
                    "If someone contacts you claiming to be from Block Parliament asking for sensitive "
                    "information, it's a scam. Report suspicious activity to "
                    <a href=twitter_url2 target="_blank" rel="noopener noreferrer">
                        "@" {CONFIG.contact.twitter}
                    </a>
                    "."
                </p>
                <p class="mt-3">
                    "Read our full " <a href="/security">"security policy"</a> " for details on how we "
                    "protect validator operations."
                </p>
            </Section>

            // Footer
            <footer class="mt-12 pt-4 border-t border-dashed border-[var(--rule)] text-center text-[var(--ink-light)] text-sm">
                <a href="/">"← back to home"</a>
            </footer>
        </main>
    }
}
