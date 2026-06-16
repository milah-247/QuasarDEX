/**
 * Thin wrapper around Stellar SDK for QuasarDEX contract calls.
 */
import { Contract, Networks, rpc, TransactionBuilder, BASE_FEE, nativeToScVal, scValToNative, Address } from "@stellar/stellar-sdk";

export const NETWORK_PASSPHRASE = Networks.TESTNET;
export const RPC_URL = process.env.NEXT_PUBLIC_RPC_URL ?? "https://soroban-testnet.stellar.org";
export const CONTRACT_ID = process.env.NEXT_PUBLIC_CONTRACT_ID ?? "";

const server = new rpc.Server(RPC_URL);

export async function simulateContractCall(
  sourcePublicKey: string,
  method: string,
  args: Parameters<typeof nativeToScVal>[0][]
) {
  const account = await server.getAccount(sourcePublicKey);
  const contract = new Contract(CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
    .addOperation(contract.call(method, ...args.map((a) => nativeToScVal(a))))
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) throw new Error(sim.error);
  return scValToNative((sim as rpc.Api.SimulateTransactionSuccessResponse).result!.retval);
}

/** Calculate price impact % for a swap */
export function calcPriceImpact(
  amountIn: bigint,
  reserveIn: bigint,
  reserveOut: bigint,
  feeBps: number
): number {
  if (reserveIn === 0n || reserveOut === 0n) return 0;
  const spotPrice = Number(reserveOut) / Number(reserveIn);
  const feeNum = 10_000 - feeBps;
  const amtWithFee = Number(amountIn) * feeNum;
  const amountOut = (amtWithFee * Number(reserveOut)) / (Number(reserveIn) * 10_000 + amtWithFee);
  const executionPrice = amountOut / Number(amountIn);
  return Math.abs((spotPrice - executionPrice) / spotPrice) * 100;
}
