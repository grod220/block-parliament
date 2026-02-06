use crate::config::CONFIG;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::api::{
    JitoMevHistory, NetworkComparison, SfdpStatus, StakewizValidator, format_lamports_to_sol, format_percent,
    format_stake,
};

/// All data needed for metrics display
#[derive(Clone, Serialize, Deserialize)]
pub struct MetricsData {
    pub validator: StakewizValidator,
    pub mev_history: Option<JitoMevHistory>,
    pub network_comp: Option<NetworkComparison>,
    pub sfdp_status: Option<SfdpStatus>,
}

/// Server function to fetch all metrics data
/// This runs on the server during SSR, avoiding CORS issues
#[server(FetchMetrics)]
pub async fn fetch_metrics() -> Result<Option<MetricsData>, ServerFnError> {
    use crate::api::{get_jito_mev_history, get_network_comparison, get_sfdp_status, get_validator_data};

    // Fetch Stakewiz data first (required)
    let Some(validator) = get_validator_data().await else {
        return Ok(None);
    };

    // Fetch additional data in parallel - each can fail independently
    let (mev_result, sfdp_result, network_result) = futures::join!(
        get_jito_mev_history(5),
        get_sfdp_status(),
        get_network_comparison(validator.skip_rate, validator.activated_stake),
    );

    Ok(Some(MetricsData {
        validator,
        mev_history: mev_result,
        network_comp: network_result,
        sfdp_status: sfdp_result,
    }))
}

/// Skeleton loading state for metrics
#[component]
fn MetricsSkeleton() -> impl IntoView {
    view! {
        <div class="space-y-4">
            // Hero APY skeleton
            <div class="border border-dashed border-[var(--rule)] p-4 text-center">
                <div class="skeleton-line">"TOTAL APY"</div>
                <div class="skeleton-line text-2xl font-bold">"\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"</div>
                <div class="skeleton-line text-sm">"\u{2591}\u{2591}\u{2591}\u{2591} staking + \u{2591}\u{2591}\u{2591}\u{2591} mev"</div>
            </div>
            // Grouped boxes skeleton
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <div class="border border-dashed border-[var(--rule)] p-3">
                    <div class="skeleton-line font-bold mb-2">"PERFORMANCE"</div>
                    <div class="skeleton-line">"\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"</div>
                    <div class="skeleton-line">"\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"</div>
                    <div class="skeleton-line">"\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"</div>
                </div>
                <div class="border border-dashed border-[var(--rule)] p-3">
                    <div class="skeleton-line font-bold mb-2">"STAKE"</div>
                    <div class="skeleton-line">"\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"</div>
                    <div class="skeleton-line">"\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"</div>
                    <div class="skeleton-line">"\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"</div>
                </div>
            </div>
        </div>
    }
}

/// Metrics component - displays validator stats
#[component]
pub fn Metrics() -> impl IntoView {
    let metrics = Resource::new(|| (), |_| fetch_metrics());

    view! {
        <Suspense fallback=move || view! { <MetricsSkeleton /> }>
            {move || {
                metrics.get().map(|result| {
                    match result {
                        Ok(Some(data)) => view! { <MetricsContent data=data /> }.into_any(),
                        Ok(None) | Err(_) => view! {
                            <div class="text-[var(--ink-light)]">
                                "Live metrics unavailable. See "
                                <a href=CONFIG.links.stakewiz>"Stakewiz"</a>
                                " for current data."
                            </div>
                        }.into_any(),
                    }
                })
            }}
        </Suspense>
    }
}

#[component]
fn MetricsContent(data: MetricsData) -> impl IntoView {
    let v = data.validator.clone();
    let status_icon = if v.delinquent { "\u{2717}" } else { "\u{2713}" };
    let status_text = if v.delinquent { "DELINQUENT" } else { "ACTIVE" };

    let version = v.version.clone();
    let ip_city = v.ip_city.clone().unwrap_or_default();
    let ip_country = v.ip_country.clone().unwrap_or_default();
    let ip_org = v.ip_org.clone().unwrap_or_default();
    let asn = v.asn.clone().unwrap_or_default();
    let client = if v.is_jito { "jito-solana" } else { "agave" };

    let has_sfdp = data.sfdp_status.as_ref().map(|s| s.is_participant).unwrap_or(false);
    let is_jito = v.is_jito;
    let mev_history = data.mev_history.clone();
    let network_comp = data.network_comp.clone();

    view! {
        <div class="space-y-4">
            // Hero APY - the number delegators care about most
            <div class="border border-dashed border-[var(--rule)] p-4 text-center">
                <div class="text-[var(--ink-light)] text-sm">"TOTAL APY"</div>
                <div class="text-2xl font-bold">{format_percent(v.total_apy, 2)}</div>
                <div class="text-sm text-[var(--ink-light)]">
                    {format_percent(v.staking_apy, 2)} " staking + "
                    {format_percent(v.jito_apy, 2)} " mev"
                </div>
            </div>

            // Status Line + Badges
            <div>
                <div>
                    <strong>{status_icon} " " {status_text}</strong>
                    " \u{00B7} v" {version}
                    " \u{00B7} rank #" {v.rank}
                    " \u{00B7} wiz " {format!("{:.0}", v.wiz_score)} "/100"
                </div>
                <div class="flex flex-wrap gap-2 mt-2">
                    {has_sfdp.then(|| view! {
                        <span class="inline-block px-2 py-0.5 text-sm border border-[var(--rule)] bg-[var(--paper)]">
                            "SFDP \u{2713}"
                        </span>
                    })}
                    {is_jito.then(|| view! {
                        <span class="inline-block px-2 py-0.5 text-sm border border-[var(--rule)] bg-[var(--paper)]">
                            "JITO-BAM \u{2713}"
                        </span>
                    })}
                    <span class="inline-block px-2 py-0.5 text-sm border border-[var(--rule)] bg-[var(--paper)]">
                        "DOUBLEZERO \u{2713}"
                    </span>
                </div>
            </div>

            // Grouped metric boxes
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                // Performance box
                <div class="border border-dashed border-[var(--rule)] p-3">
                    <div class="font-bold mb-2 text-sm">"PERFORMANCE"</div>
                    <div>"Vote Success  " {format_percent(v.vote_success, 2)}</div>
                    <div>"Skip Rate     " {format_percent(v.skip_rate, 2)}</div>
                    <div>"Uptime        " {format_percent(v.uptime, 1)}</div>
                    <div>"Credit Ratio  " {format_percent(v.credit_ratio, 2)}</div>
                    {network_comp.map(|nc| view! {
                        <div class="mt-2 text-sm text-[var(--ink-light)]">
                            "vs network (" {nc.total_validators} " validators)"
                            <br />
                            "skip: top " {nc.skip_rate_percentile} "%"
                            " \u{00B7} stake: top " {nc.stake_percentile} "%"
                        </div>
                    })}
                </div>

                // Stake & Commission box
                <div class="border border-dashed border-[var(--rule)] p-3">
                    <div class="font-bold mb-2 text-sm">"STAKE & FEES"</div>
                    <div>"Stake         " {format_stake(v.activated_stake)} " SOL"</div>
                    <div>"Commission    " {v.commission} "%"</div>
                    <div>"Jito MEV Fee  " {format!("{:.1}", v.jito_commission_bps as f64 / 100.0)} "%"</div>
                </div>
            </div>

            // MEV Rewards History
            <div>
                <strong>"MEV REWARDS"</strong>
                {match mev_history {
                    Some(mh) if !mh.epochs.is_empty() => {
                        let epochs = mh.epochs.clone();
                        let count = epochs.len();
                        view! {
                            " (last " {count} " epochs)"
                            <div class="mt-1 text-sm font-mono">
                                {epochs.into_iter().rev().take(5).collect::<Vec<_>>().into_iter().rev().map(|e| {
                                    let epoch = e.epoch;
                                    let rewards = format_lamports_to_sol(e.get_mev_rewards(), 4);
                                    view! {
                                        <div class="text-[var(--ink-light)]">
                                            "E" {epoch} ": " {rewards} " SOL"
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    },
                    _ => view! {
                        <div class="mt-1 text-sm text-[var(--ink-light)]">
                            "See "
                            <a href=CONFIG.links.jito>"Jito"</a>
                            " for MEV reward details"
                        </div>
                    }.into_any(),
                }}
            </div>

            // Infrastructure
            <div class="text-[var(--ink-light)] text-sm">
                {ip_city} ", " {ip_country} " \u{00B7} " {ip_org}
                <br />
                {client} " \u{00B7} ASN " {asn} " \u{00B7} epoch " {v.epoch}
            </div>
        </div>
    }
}
