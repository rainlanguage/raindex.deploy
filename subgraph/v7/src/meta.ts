import { MetaV1_2 } from "../generated/Raindex/Raindex";
import { Order } from "../generated/schema";
import { makeOrderId } from "./order";

export function handleMeta(event: MetaV1_2): void {
  // The order should already exist by the time the MetaV1 event is handled
  let order = Order.load(makeOrderId(event.address, event.params.subject));
  if (order != null) {
    order.meta = event.params.meta;
    order.save();
  }
}
