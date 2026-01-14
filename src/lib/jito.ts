import { config } from "./config";

// Jito Block Engine API
const JITO_API_BASE = "https://kobe.mainnet.jito.network";

export interface JitoValidatorInfo {
  vote_account: string;
  running_jito: boolean;
  mev_commission_bps: number;
}

export interface JitoEpochReward {
  epoch: number;
  mev_rewards: number; // in lamports
  total_rewards: number; // inflation + MEV in lamports
  mev_commission_earned: number; // in lamports
}

export interface JitoMevHistory {
  vote_account: string;
  epochs: JitoEpochReward[];
}

// API response types
interface JitoValidatorApiResponse {
  vote_account?: string;
  running_jito?: boolean;
  mev_commission_bps?: number;
}

interface JitoEpochApiResponse {
  epoch?: number;
  mev_rewards?: number;
  MEV_rewards?: number;
  total_rewards?: number;
  mev_commission_earned?: number;
  commission_earned?: number;
}

// Cache for 5 minutes
let validatorCache: { data: JitoValidatorInfo | null; timestamp: number } | null = null;
let historyCache: { data: JitoMevHistory | null; timestamp: number } | null = null;
const CACHE_TTL = 5 * 60 * 1000;

/**
 * Fetch validator info from Jito API to check BAM/Jito status
 */
export async function getJitoValidatorInfo(): Promise<JitoValidatorInfo | null> {
  if (validatorCache && Date.now() - validatorCache.timestamp < CACHE_TTL) {
    return validatorCache.data;
  }

  try {
    const response = await fetch(`${JITO_API_BASE}/api/v1/validators/${config.voteAccount}`, {
      headers: { Accept: "application/json" },
    });

    if (!response.ok) {
      console.error("Jito validator API error:", response.status);
      return validatorCache?.data ?? null;
    }

    const data = (await response.json()) as JitoValidatorApiResponse;

    const result: JitoValidatorInfo = {
      vote_account: data.vote_account ?? config.voteAccount,
      running_jito: data.running_jito ?? false,
      mev_commission_bps: data.mev_commission_bps ?? 0,
    };

    validatorCache = { data: result, timestamp: Date.now() };
    return result;
  } catch (error) {
    console.error("Failed to fetch Jito validator info:", error);
    return validatorCache?.data ?? null;
  }
}

/**
 * Fetch MEV rewards history from Jito API
 * Returns the last N epochs of MEV reward data
 */
export async function getJitoMevHistory(epochCount = 10): Promise<JitoMevHistory | null> {
  if (historyCache && Date.now() - historyCache.timestamp < CACHE_TTL) {
    return historyCache.data;
  }

  try {
    // Jito API returns MEV history at the validator endpoint directly
    const response = await fetch(`${JITO_API_BASE}/api/v1/validators/${config.voteAccount}`, {
      headers: { Accept: "application/json" },
    });

    if (!response.ok) {
      console.error("Jito MEV history API error:", response.status);
      return historyCache?.data ?? null;
    }

    const data = (await response.json()) as JitoEpochApiResponse[] | { epochs?: JitoEpochApiResponse[] };

    // Handle array response or object with epochs array
    const epochsArray: JitoEpochApiResponse[] = Array.isArray(data) ? data : (data.epochs ?? []);

    const epochs: JitoEpochReward[] = epochsArray.slice(-epochCount).map((e) => ({
      epoch: Number(e.epoch ?? 0),
      mev_rewards: Number(e.mev_rewards ?? e.MEV_rewards ?? 0),
      total_rewards: Number(e.total_rewards ?? 0),
      mev_commission_earned: Number(e.mev_commission_earned ?? e.commission_earned ?? 0),
    }));

    const result: JitoMevHistory = {
      vote_account: config.voteAccount,
      epochs,
    };

    historyCache = { data: result, timestamp: Date.now() };
    return result;
  } catch (error) {
    console.error("Failed to fetch Jito MEV history:", error);
    return historyCache?.data ?? null;
  }
}

/**
 * Format lamports to SOL with appropriate precision
 */
export function formatLamportsToSol(lamports: number, decimals = 4): string {
  const sol = lamports / 1_000_000_000;
  if (sol === 0) return "0";
  if (sol < 0.0001) return "<0.0001";
  return sol.toLocaleString("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: decimals,
  });
}

/**
 * Calculate total MEV rewards across all epochs
 */
export function calculateTotalMevRewards(history: JitoMevHistory): number {
  return history.epochs.reduce((sum, e) => sum + e.mev_rewards, 0);
}
