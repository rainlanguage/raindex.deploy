import { ethereum } from "@graphprotocol/graph-ts";
import { Raindex } from "../generated/schema";

export function createRaindexEntity(event: ethereum.Event): void {
  let raindex = Raindex.load(event.address);
  if (!raindex) {
    raindex = new Raindex(event.address);
    raindex.save();
  }
}
