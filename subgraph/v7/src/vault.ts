import { Bytes, crypto } from "@graphprotocol/graph-ts";
import { Vault } from "../generated/schema";
import { getERC20Entity } from "./erc20";
import { Float, getCalculator } from "./float";

export type VaultId = Bytes;

export function vaultEntityId(
  raindex: Bytes,
  owner: Bytes,
  vaultId: VaultId,
  token: Bytes
): Bytes {
  let bytes = raindex.concat(owner.concat(token.concat(vaultId)));
  return Bytes.fromByteArray(crypto.keccak256(bytes));
}

export function createEmptyVault(
  raindex: Bytes,
  owner: Bytes,
  vaultId: VaultId,
  token: Bytes
): Vault {
  let vault = new Vault(vaultEntityId(raindex, owner, vaultId, token));
  vault.raindex = raindex;
  vault.vaultId = vaultId;
  vault.token = getERC20Entity(token);
  vault.owner = owner;
  vault.balance = Bytes.fromHexString(
    "0x0000000000000000000000000000000000000000000000000000000000000000"
  );
  vault.save();
  return vault;
}

export function getVault(
  raindex: Bytes,
  owner: Bytes,
  vaultId: Bytes,
  token: Bytes
): Vault {
  let vault = Vault.load(vaultEntityId(raindex, owner, vaultId, token));
  if (vault == null) {
    vault = createEmptyVault(raindex, owner, vaultId, token);
  }
  return vault;
}

export class VaultBalanceChange {
  oldVaultBalance: Float;
  newVaultBalance: Float;
}

export function handleVaultBalanceChange(
  raindex: Bytes,
  vaultId: Bytes,
  token: Bytes,
  amount: Float,
  owner: Bytes
): VaultBalanceChange {
  const calculator = getCalculator();

  let vault = getVault(raindex, owner, vaultId, token);
  let oldVaultBalance = vault.balance;
  vault.balance = calculator.add(oldVaultBalance, amount);
  vault.save();

  return {
    oldVaultBalance,
    newVaultBalance: vault.balance,
  };
}
