import { config } from "./config";

// Public Solana RPC endpoint
// Note: Public RPC may block browser CORS requests. For production,
// consider using a dedicated RPC provider (Helius, QuickNode, etc.)
const RPC_ENDPOINT = "https://api.mainnet-beta.solana.com";

// SFDP API endpoint
const SFDP_API = "https://api.solana.org/api/community/v1/sfdp_participants";

export interface NetworkComparison {
  totalValidators: number;
  skipRatePercentile: number; // Lower is better (1 = best)
  voteSuccessPercentile: number; // Lower is better
  stakePercentile: number; // Lower is better (1 = most stake)
  networkAvgSkipRate: number;
  networkAvgVoteSuccess: number;
}

export interface SfdpStatus {
  isParticipant: boolean;
  programName?: string;
  status?: string;
  onboardingDate?: string;
}

// API response types
interface RpcResponse<T> {
  result?: T;
  error?: { message: string };
}

interface SfdpParticipant {
  identity?: string;
  vote_account?: string;
  program_name?: string;
  status?: string;
  onboarding_date?: string;
}

// Cache settings
const CACHE_TTL = 5 * 60 * 1000;
let networkCache: { data: NetworkComparison | null; timestamp: number } | null = null;
let sfdpCache: { data: SfdpStatus | null; timestamp: number } | null = null;

/**
 * Make a JSON-RPC call to Solana
 */
async function rpcCall<T>(method: string, params: unknown[]): Promise<T> {
  const response = await fetch(RPC_ENDPOINT, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method,
      params,
    }),
  });

  if (!response.ok) {
    throw new Error(`RPC error: ${response.status}`);
  }

  const json = (await response.json()) as RpcResponse<T>;
  if (json.error) {
    throw new Error(`RPC error: ${json.error.message}`);
  }

  return json.result as T;
}

/**
 * Fetch network comparison data using getVoteAccounts
 * Calculates percentile rankings for this validator
 */
export async function getNetworkComparison(
  currentSkipRate: number,
  currentVoteSuccess: number,
  currentStake: number
): Promise<NetworkComparison | null> {
  if (networkCache && Date.now() - networkCache.timestamp < CACHE_TTL) {
    return networkCache.data;
  }

  try {
    const result = await rpcCall<{
      current: {
        votePubkey: string;
        activatedStake: number;
        commission: number;
        lastVote: number;
        rootSlot: number;
      }[];
    }>("getVoteAccounts", [{ commitment: "confirmed" }]);

    const validators = result.current ?? [];
    const totalValidators = validators.length;

    if (totalValidators === 0) {
      return null;
    }

    // Estimate percentile based on stake rank
    const stakes = validators.map((v) => v.activatedStake).sort((a, b) => b - a);
    const stakeRank = stakes.findIndex((s) => s <= currentStake * 1_000_000_000) + 1;
    const stakePercentile = Math.round((stakeRank / totalValidators) * 100);

    // Network averages (typical values from reports)
    const networkAvgSkipRate = 0.2;
    const networkAvgVoteSuccess = 99.5;

    // Estimate percentiles based on current values vs network average
    const skipRatePercentile =
      currentSkipRate <= networkAvgSkipRate
        ? Math.round((1 - (currentSkipRate / networkAvgSkipRate) * 0.5) * 50)
        : Math.round(50 + (currentSkipRate / networkAvgSkipRate - 1) * 50);

    const voteSuccessPercentile =
      currentVoteSuccess >= networkAvgVoteSuccess
        ? Math.round((1 - (currentVoteSuccess - networkAvgVoteSuccess) / (100 - networkAvgVoteSuccess)) * 25)
        : Math.round(25 + ((networkAvgVoteSuccess - currentVoteSuccess) / networkAvgVoteSuccess) * 75);

    const data: NetworkComparison = {
      totalValidators,
      skipRatePercentile: Math.max(1, Math.min(100, skipRatePercentile)),
      voteSuccessPercentile: Math.max(1, Math.min(100, voteSuccessPercentile)),
      stakePercentile: Math.max(1, Math.min(100, stakePercentile)),
      networkAvgSkipRate,
      networkAvgVoteSuccess,
    };

    networkCache = { data, timestamp: Date.now() };
    return data;
  } catch (error) {
    console.error("Failed to fetch network comparison:", error);
    return networkCache?.data ?? null;
  }
}

/**
 * Check SFDP participation status
 */
export async function getSfdpStatus(): Promise<SfdpStatus | null> {
  if (sfdpCache && Date.now() - sfdpCache.timestamp < CACHE_TTL) {
    return sfdpCache.data;
  }

  try {
    const response = await fetch(SFDP_API, {
      headers: { Accept: "application/json" },
    });

    if (!response.ok) {
      console.error("SFDP API error:", response.status);
      return null;
    }

    const participants = (await response.json()) as SfdpParticipant[];

    // Search for our identity in the participants list
    const ourEntry = Array.isArray(participants)
      ? participants.find((p) => p.identity === config.identity || p.vote_account === config.voteAccount)
      : null;

    if (!ourEntry) {
      return null;
    }

    const data: SfdpStatus = {
      isParticipant: true,
      programName: ourEntry.program_name ?? "Mainnet Beta",
      status: ourEntry.status ?? "Active",
      onboardingDate: ourEntry.onboarding_date ?? "",
    };

    sfdpCache = { data, timestamp: Date.now() };
    return data;
  } catch (error) {
    console.error("Failed to fetch SFDP status:", error);
    return null;
  }
}
