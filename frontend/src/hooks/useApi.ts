'use client';

import useSWR, { type SWRConfiguration, type SWRResponse } from 'swr';
import * as api from '@/lib/api';
import { FETCH_TIMEOUT_MS, POLL_MS } from '@/lib/chainStatus';
import type {
  AdmittedSet,
  MultisigAccount,
  PerpsInfo,
  RandPrice,
  ApiKey,
  BlockDetail,
  BlockSummary,
  ApprovedToken,
  BridgeAssetActivity,
  BridgeState,
  ChainView,
  Health,
  NetworkStats,
  NodeInfo,
  ProverView,
  Note,
  Paginated,
  ProgramDetail,
  ProgramSummary,
  SearchResult,
  Supply,
  TokenDetail,
  TokenList,
  TransactionDetail,
  TransactionKind,
  TransactionSummary,
  User,
  Validator,
  ValidatorDetail,
} from '@/types';

const defaultConfig: SWRConfiguration = {
  revalidateOnFocus: false,
  revalidateOnReconnect: true,
  dedupingInterval: 2000,
  shouldRetryOnError: (err: unknown) => !api.isNotFoundError(err),
};

/** Detail pages need a stable "this thing does not exist" signal. */
export function isNotFound(error: unknown): boolean {
  return api.isNotFoundError(error);
}

// ---------------------------------------------------------------------------
// Health & stats
// ---------------------------------------------------------------------------

export function useHealth(config?: SWRConfiguration): SWRResponse<Health> {
  return useSWR<Health>('health', api.getHealth, {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

/** GET /api/v1/chain every 15 s (site-sync design §3); SWR keeps the last good view on error. */
export function useChain(config?: SWRConfiguration): SWRResponse<ChainView> {
  return useSWR<ChainView>('chain', () => api.getChain(FETCH_TIMEOUT_MS), {
    ...defaultConfig,
    refreshInterval: POLL_MS,
    revalidateOnFocus: true,
    shouldRetryOnError: false,
    ...config,
  });
}

export function useStats(config?: SWRConfiguration): SWRResponse<NetworkStats> {
  return useSWR<NetworkStats>('stats', api.getStats, {
    ...defaultConfig,
    refreshInterval: 10_000,
    ...config,
  });
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

export function useBlocks(
  page = 1,
  limit = 25,
  proposer?: string,
  config?: SWRConfiguration
): SWRResponse<Paginated<BlockSummary>> {
  return useSWR<Paginated<BlockSummary>>(
    ['blocks', page, limit, proposer ?? null],
    () => api.getBlocks(page, limit, proposer),
    { ...defaultConfig, keepPreviousData: true, ...config }
  );
}

export function useLatestBlocks(
  limit = 10,
  config?: SWRConfiguration
): SWRResponse<BlockSummary[]> {
  return useSWR<BlockSummary[]>(
    ['blocks/latest', limit],
    () => api.getLatestBlocks(limit),
    { ...defaultConfig, refreshInterval: 15_000, ...config }
  );
}

export function useBlock(
  id: string | number | null,
  config?: SWRConfiguration
): SWRResponse<BlockDetail> {
  return useSWR<BlockDetail>(
    id === null || id === '' ? null : ['block', String(id)],
    () => api.getBlock(id as string | number),
    { ...defaultConfig, ...config }
  );
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

export function useTransactions(
  page = 1,
  limit = 25,
  kind?: TransactionKind | null,
  filter?: { validator?: string | null; program?: string | null; height?: number | null },
  config?: SWRConfiguration
): SWRResponse<Paginated<TransactionSummary>> {
  const validator = filter?.validator ?? null;
  const program = filter?.program ?? null;
  const height = filter?.height ?? null;
  return useSWR<Paginated<TransactionSummary>>(
    ['transactions', page, limit, kind ?? null, validator, program, height],
    () => api.getTransactions({ page, limit, kind, validator, program, height }),
    { ...defaultConfig, keepPreviousData: true, ...config }
  );
}

export function useLatestTransactions(
  limit = 10,
  config?: SWRConfiguration
): SWRResponse<TransactionSummary[]> {
  return useSWR<TransactionSummary[]>(
    ['transactions/latest', limit],
    () => api.getLatestTransactions(limit),
    { ...defaultConfig, refreshInterval: 15_000, ...config }
  );
}

export function useTransaction(
  hash: string | null,
  config?: SWRConfiguration
): SWRResponse<TransactionDetail> {
  return useSWR<TransactionDetail>(
    hash ? ['transaction', hash] : null,
    () => api.getTransaction(hash as string),
    { ...defaultConfig, ...config }
  );
}

// ---------------------------------------------------------------------------
// Notes, bridge, supply
// ---------------------------------------------------------------------------

export function useNotes(
  page = 1,
  limit = 25,
  config?: SWRConfiguration
): SWRResponse<Paginated<Note>> {
  return useSWR<Paginated<Note>>(['notes', page, limit], () => api.getNotes(page, limit), {
    ...defaultConfig,
    keepPreviousData: true,
    refreshInterval: 15_000,
    ...config,
  });
}

export function useNote(
  id: string | null,
  config?: SWRConfiguration
): SWRResponse<Note> {
  return useSWR<Note>(id ? ['note', id] : null, () => api.getNote(id as string), {
    ...defaultConfig,
    ...config,
  });
}

export function useBridge(config?: SWRConfiguration): SWRResponse<BridgeState> {
  return useSWR<BridgeState>('bridge', api.getBridge, {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

export function useBridgeAssets(config?: SWRConfiguration): SWRResponse<BridgeAssetActivity[]> {
  return useSWR<BridgeAssetActivity[]>('bridge-assets', api.getBridgeAssets, {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

export function useBridgeTokens(config?: SWRConfiguration): SWRResponse<ApprovedToken[]> {
  return useSWR<ApprovedToken[]>('bridge-tokens', api.getBridgeTokens, {
    ...defaultConfig,
    revalidateOnFocus: false,
    ...config,
  });
}

/** `null` when the node serves no supply audit (404). */
export function useSupply(config?: SWRConfiguration): SWRResponse<Supply | null> {
  return useSWR<Supply | null>(
    'supply',
    async () => {
      try {
        return await api.getSupply();
      } catch (err) {
        if (api.isNotFoundError(err)) return null;
        throw err;
      }
    },
    { ...defaultConfig, refreshInterval: 30_000, ...config }
  );
}

/** A read that 404s on a chain without the section answers `null` instead of an error. */
function orNull<T>(fetch: () => Promise<T>): () => Promise<T | null> {
  return async () => {
    try {
      return await fetch();
    } catch (err) {
      if (api.isNotFoundError(err)) return null;
      throw err;
    }
  };
}

/** The perps exchange (fullnode v0.8.0); `null` on a chain without a `perps` section. */
export function usePerps(config?: SWRConfiguration): SWRResponse<PerpsInfo | null> {
  return useSWR<PerpsInfo | null>('perps', orNull(api.getPerps), {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

/** The voted RAND/USD price (fullnode v0.8.0); `null` on a chain without `fees.usd_subsidy`. */
export function useRandPrice(config?: SWRConfiguration): SWRResponse<RandPrice | null> {
  return useSWR<RandPrice | null>('price', orNull(api.getRandPrice), {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

/** One multisig account as the node holds it now; `null` for an unknown id. */
export function useMultisig(id: string | null, config?: SWRConfiguration): SWRResponse<MultisigAccount | null> {
  return useSWR<MultisigAccount | null>(
    id ? ['multisig', id] : null,
    orNull(() => api.getMultisig(id as string)),
    { ...defaultConfig, ...config }
  );
}

// ---------------------------------------------------------------------------
// RPL tokens
// ---------------------------------------------------------------------------

export function useTokens(config?: SWRConfiguration): SWRResponse<TokenList> {
  return useSWR<TokenList>('tokens', api.getTokens, {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

export function useToken(
  id: string | number | null,
  config?: SWRConfiguration
): SWRResponse<TokenDetail> {
  return useSWR<TokenDetail>(
    id === null || id === '' ? null : ['token', String(id)],
    () => api.getToken(id as string | number),
    { ...defaultConfig, ...config }
  );
}

// ---------------------------------------------------------------------------
// Validators
// ---------------------------------------------------------------------------

export function useAdmittedValidators(config?: SWRConfiguration): SWRResponse<AdmittedSet> {
  return useSWR<AdmittedSet>('validators-admitted', api.getAdmittedValidators, {
    ...defaultConfig,
    refreshInterval: 60_000,
    ...config,
  });
}

export function useValidators(config?: SWRConfiguration): SWRResponse<Validator[]> {
  return useSWR<Validator[]>('validators', api.getValidators, {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

export function useValidator(
  address: string | null,
  config?: SWRConfiguration
): SWRResponse<ValidatorDetail> {
  return useSWR<ValidatorDetail>(
    address ? ['validator', address] : null,
    () => api.getValidator(address as string),
    { ...defaultConfig, ...config }
  );
}

// ---------------------------------------------------------------------------
// Programs
// ---------------------------------------------------------------------------

export function usePrograms(
  page = 1,
  limit = 25,
  config?: SWRConfiguration
): SWRResponse<Paginated<ProgramSummary>> {
  return useSWR<Paginated<ProgramSummary>>(
    ['programs', page, limit],
    () => api.getPrograms(page, limit),
    { ...defaultConfig, keepPreviousData: true, ...config }
  );
}

export function useProgram(
  id: string | null,
  config?: SWRConfiguration
): SWRResponse<ProgramDetail> {
  return useSWR<ProgramDetail>(
    id ? ['program', id] : null,
    () => api.getProgram(id as string),
    { ...defaultConfig, ...config }
  );
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

export function useProvers(config?: SWRConfiguration): SWRResponse<ProverView[]> {
  return useSWR<ProverView[]>('provers', api.getProvers, {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

export function useNodes(config?: SWRConfiguration): SWRResponse<NodeInfo[]> {
  return useSWR<NodeInfo[]>('nodes', api.getNodes, {
    ...defaultConfig,
    refreshInterval: 30_000,
    ...config,
  });
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

export function useSearch(
  q: string | null,
  config?: SWRConfiguration
): SWRResponse<SearchResult[]> {
  return useSWR<SearchResult[]>(
    q && q.trim() !== '' ? ['search', q.trim()] : null,
    () => api.search((q as string).trim()),
    { ...defaultConfig, ...config }
  );
}

// ---------------------------------------------------------------------------
// Accounts and API keys
// ---------------------------------------------------------------------------

/** The signed-in user, or `null` when there is no session. Never retries a 401. */
export function useMe(config?: SWRConfiguration): SWRResponse<User | null> {
  return useSWR<User | null>(
    'me',
    async () => {
      try {
        return await api.getMe();
      } catch (err) {
        if (err instanceof api.ApiError && err.status === 401) return null;
        throw err;
      }
    },
    { ...defaultConfig, shouldRetryOnError: false, ...config }
  );
}

export function useApiKeys(enabled: boolean, config?: SWRConfiguration): SWRResponse<ApiKey[]> {
  return useSWR<ApiKey[]>(enabled ? 'keys' : null, api.listKeys, {
    ...defaultConfig,
    shouldRetryOnError: false,
    ...config,
  });
}
