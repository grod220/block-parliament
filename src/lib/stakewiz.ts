import { config } from "./config";

export interface StakewizValidator {
  rank: number;
  identity: string;
  vote_identity: string;
  last_vote: number;
  root_slot: number;
  credits: number;
  epoch_credits: number;
  activated_stake: number;
  version: string;
  delinquent: boolean;
  skip_rate: number;
  name: string;
  description: string;
  commission: number;
  is_jito: boolean;
  jito_commission_bps: number;
  vote_success: number;
  wiz_score: number;
  uptime: number;
  ip_city: string;
  ip_country: string;
  ip_org: string;
  epoch: number;
  apy_estimate: number;
  staking_apy: number;
  jito_apy: number;
  total_apy: number;
  credit_ratio: number;
  stake_ratio: number;
  stake_weight: number;
  asn: string;
}

// Cache for 5 minutes
let cache: { data: StakewizValidator; timestamp: number } | null = null;
const CACHE_TTL = 5 * 60 * 1000;

function isStakewizValidator(data: unknown): data is StakewizValidator {
  return typeof data === "object" && data !== null && "vote_identity" in data && "activated_stake" in data;
}

export async function getValidatorData(): Promise<StakewizValidator | null> {
  // Check cache
  if (cache && Date.now() - cache.timestamp < CACHE_TTL) {
    return cache.data;
  }

  try {
    const response = await fetch(`https://api.stakewiz.com/validator/${config.voteAccount}`, {
      headers: {
        Accept: "application/json",
      },
    });

    if (!response.ok) {
      console.error("Stakewiz API error:", response.status);
      return cache?.data ?? null;
    }

    const data: unknown = await response.json();

    // Stakewiz returns `false` for unknown validators
    if (!isStakewizValidator(data)) {
      console.error("Validator not found on Stakewiz");
      return cache?.data ?? null;
    }

    cache = { data, timestamp: Date.now() };
    return data;
  } catch (error) {
    console.error("Failed to fetch Stakewiz data:", error);
    return cache?.data ?? null;
  }
}

// Format stake in SOL with commas
export function formatStake(stake: number): string {
  return new Intl.NumberFormat("en-US", {
    maximumFractionDigits: 0,
  }).format(stake);
}

// Format percentage
export function formatPercent(value: number, decimals = 2): string {
  return value.toFixed(decimals) + "%";
}
