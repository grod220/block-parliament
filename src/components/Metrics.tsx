import { useEffect, useState } from "react";
import { getValidatorData, formatStake, formatPercent, type StakewizValidator } from "../lib/stakewiz";
import { getJitoMevHistory, formatLamportsToSol, type JitoMevHistory } from "../lib/jito";
import { getNetworkComparison, getSfdpStatus, type NetworkComparison, type SfdpStatus } from "../lib/solana-rpc";
import { config } from "../lib/config";

export function Metrics() {
  const [data, setData] = useState<StakewizValidator | null>(null);
  const [mevHistory, setMevHistory] = useState<JitoMevHistory | null>(null);
  const [networkComp, setNetworkComp] = useState<NetworkComparison | null>(null);
  const [sfdpStatus, setSfdpStatus] = useState<SfdpStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  useEffect(() => {
    async function fetchAllData() {
      // Fetch StakeWiz data first (primary source - required)
      const stakewizData = await getValidatorData();
      setData(stakewizData);

      if (!stakewizData) {
        setError(true);
        setLoading(false);
        return;
      }

      // Fetch additional data in parallel - each can fail independently
      const [mevResult, sfdpResult, networkResult] = await Promise.allSettled([
        getJitoMevHistory(5),
        getSfdpStatus(),
        getNetworkComparison(stakewizData.skip_rate, stakewizData.vote_success, stakewizData.activated_stake),
      ]);

      setMevHistory(mevResult.status === "fulfilled" ? mevResult.value : null);
      setSfdpStatus(sfdpResult.status === "fulfilled" ? sfdpResult.value : null);
      setNetworkComp(networkResult.status === "fulfilled" ? networkResult.value : null);
      setLoading(false);
    }

    void fetchAllData();
  }, []);

  if (loading) {
    return <div className="text-[var(--ink-light)]">Loading metrics...</div>;
  }

  if (error || !data) {
    return (
      <div className="text-[var(--ink-light)]">
        Live metrics unavailable. See <a href={config.links.stakewiz}>Stakewiz</a> for current data.
      </div>
    );
  }

  const statusIcon = data.delinquent ? "✗" : "✓";
  const statusText = data.delinquent ? "DELINQUENT" : "ACTIVE";

  return (
    <div className="space-y-4">
      {/* Status Line */}
      <div>
        <strong>
          {statusIcon} {statusText}
        </strong>{" "}
        · v{data.version} · rank #{data.rank} · wiz {data.wiz_score.toFixed(0)}
        /100
      </div>

      {/* Badges / Trust Indicators */}
      <div className="flex flex-wrap gap-2">
        {sfdpStatus?.isParticipant && (
          <span className="inline-block px-2 py-0.5 text-sm border border-[var(--rule)] bg-[var(--paper)]">SFDP ✓</span>
        )}
        {data.is_jito && (
          <span className="inline-block px-2 py-0.5 text-sm border border-[var(--rule)] bg-[var(--paper)]">
            JITO-BAM ✓
          </span>
        )}
      </div>

      {/* Stake & Commission */}
      <div>
        <strong>STAKE</strong> {formatStake(data.activated_stake)} SOL
        <br />
        <strong>COMMISSION</strong> {data.commission}%
        <br />
        <strong>JITO MEV FEE</strong> {data.jito_commission_bps / 100}%
      </div>

      {/* Performance */}
      <div>
        <strong>VOTE SUCCESS</strong> {formatPercent(data.vote_success)}
        <br />
        <strong>SKIP RATE</strong> {formatPercent(data.skip_rate)}
        <br />
        <strong>UPTIME</strong> {formatPercent(data.uptime, 1)}
        <br />
        <strong>CREDIT RATIO</strong> {formatPercent(data.credit_ratio)}
      </div>

      {/* Network Comparison */}
      {networkComp && (
        <div className="text-[var(--ink-light)]">
          <strong className="text-[var(--ink)]">VS NETWORK</strong> ({networkComp.totalValidators} validators)
          <br />
          Skip rate: top {networkComp.skipRatePercentile}%{" · "}
          Stake: top {networkComp.stakePercentile}%
        </div>
      )}

      {/* APY */}
      <div>
        <strong>APY (staking)</strong> {formatPercent(data.staking_apy)}
        <br />
        <strong>APY (jito mev)</strong> {formatPercent(data.jito_apy)}
        <br />
        <strong>APY (total)</strong> {formatPercent(data.total_apy)}
      </div>

      {/* MEV Rewards History */}
      <div>
        <strong>MEV REWARDS</strong>
        {mevHistory && mevHistory.epochs.length > 0 ? (
          <>
            {" "}
            (last {mevHistory.epochs.length} epochs)
            <div className="mt-1 text-sm font-mono">
              {mevHistory.epochs.slice(-5).map((e) => (
                <div key={e.epoch} className="text-[var(--ink-light)]">
                  E{e.epoch}: {formatLamportsToSol(e.mev_rewards)} SOL
                </div>
              ))}
            </div>
          </>
        ) : (
          <div className="mt-1 text-sm text-[var(--ink-light)]">
            See <a href={config.links.jito}>Jito</a> for MEV reward details
          </div>
        )}
      </div>

      {/* Infrastructure */}
      <div className="text-[var(--ink-light)]">
        {data.ip_city}, {data.ip_country} · {data.ip_org}
        <br />
        {data.is_jito ? "jito-solana" : "agave"} · ASN {data.asn} · epoch {data.epoch}
      </div>
    </div>
  );
}
