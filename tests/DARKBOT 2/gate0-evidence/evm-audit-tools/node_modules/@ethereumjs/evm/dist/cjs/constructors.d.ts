import { EVM } from './index.ts';
import type { EVMOpts } from './index.ts';
/**
 * Async factory for initializing an {@link EVM} with sensible defaults.
 *
 * Supplies a {@link NobleBN254} precompile backend, mainnet {@link @ethereumjs/common!Common},
 * {@link EVMMockBlockchain}, and {@link @ethereumjs/statemanager!SimpleStateManager} when not provided.
 *
 * @param createOpts EVM configuration
 * @returns Initialized EVM instance
 */
export declare function createEVM(createOpts?: EVMOpts): Promise<EVM>;
//# sourceMappingURL=constructors.d.ts.map