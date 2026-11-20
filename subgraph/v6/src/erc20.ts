import { Address, BigInt, Bytes } from "@graphprotocol/graph-ts";
import { ERC20 as ERC20Entity } from "../generated/schema";
import { ERC20 } from "../generated/Raindex/ERC20";

export function createERC20Entity(address: Bytes): void {
  let erc20 = ERC20.bind(Address.fromBytes(address));
  let entity = new ERC20Entity(address);
  entity.address = address;
  entity.name = erc20.try_name().reverted ? null : erc20.name();
  entity.symbol = erc20.try_symbol().reverted ? null : erc20.symbol();
  entity.decimals = erc20.try_decimals().reverted
    ? null
    : BigInt.fromI32(erc20.decimals());
  entity.save();
}

export function getERC20Entity(address: Bytes): Bytes {
  let entity = ERC20Entity.load(address);
  if (entity == null) {
    createERC20Entity(address);
    return address;
  } else {
    return address;
  }
}
