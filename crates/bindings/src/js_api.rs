use crate::IRaindexV7::{
    EvaluableV4, OrderV4, QuoteV3, SignedContextV2, TakeOrderConfigV5, TakeOrdersConfigV6, IOV2,
};
use wasm_bindgen_utils::{impl_custom_tsify, impl_wasm_traits, prelude::*};

impl_wasm_traits!(IOV2);
impl_wasm_traits!(QuoteV3);
impl_wasm_traits!(OrderV4);
impl_wasm_traits!(EvaluableV4);
impl_wasm_traits!(SignedContextV2);
impl_wasm_traits!(TakeOrderConfigV5);
impl_wasm_traits!(TakeOrdersConfigV6);

impl_custom_tsify!(
    IOV2,
    "export interface IOV2 {
    token: string;
    vaultId: string;
}"
);
impl_custom_tsify!(
    QuoteV3,
    "export interface QuoteV3 {
    order: OrderV4;
    inputIOIndex: string;
    outputIOIndex: string;
    signedContext: SignedContextV2[];
}"
);
impl_custom_tsify!(
    OrderV4,
    "export interface OrderV4 {
    owner: string;
    evaluable: EvaluableV4;
    validInputs: IOV2[];
    validOutputs: IOV2[];
    nonce: string;
}"
);
impl_custom_tsify!(
    EvaluableV4,
    "export interface EvaluableV4 {
    interpreter: string;
    store: string;
    bytecode: string;
}"
);
impl_custom_tsify!(
    SignedContextV2,
    "export interface SignedContextV2 {
    signer: string;
    domainSeparator: string;
    context: string[];
    signature: string;
}"
);
impl_custom_tsify!(
    TakeOrderConfigV5,
    "export interface TakeOrderConfigV5 {
    order: OrderV4;
    inputIOIndex: string;
    outputIOIndex: string;
    signedContext: SignedContextV2[];
}"
);
impl_custom_tsify!(
    TakeOrdersConfigV6,
    "export interface TakeOrdersConfigV6 {
    minimumIO: string;
    maximumIO: string;
    maximumIORatio: string;
    IOIsInput: string;
    orders: TakeOrderConfigV5[];
    data: string;
}"
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use wasm_bindgen_test::wasm_bindgen_test;
    use wasm_bindgen_utils::prelude::js_sys::JsString;

    #[wasm_bindgen_test]
    fn test_io_tsify() {
        let js_io = to_js_value(&IOV2::default()).unwrap();
        // validate serialized props match the tsify definition
        assert!(JsString::from_str("token").unwrap().js_in(&js_io));
        assert!(JsString::from_str("vaultId").unwrap().js_in(&js_io));
    }

    #[wasm_bindgen_test]
    fn test_quote_tsify() {
        let js_quote = to_js_value(&QuoteV3::default()).unwrap();
        // validate serialized props match the tsify definition
        assert!(JsString::from_str("order").unwrap().js_in(&js_quote));
        assert!(JsString::from_str("inputIOIndex").unwrap().js_in(&js_quote));
        assert!(JsString::from_str("outputIOIndex")
            .unwrap()
            .js_in(&js_quote));
        assert!(JsString::from_str("signedContext")
            .unwrap()
            .js_in(&js_quote));
    }

    #[wasm_bindgen_test]
    fn test_orderv4_tsify() {
        let js_order = to_js_value(&OrderV4::default()).unwrap();
        // validate serialized props match the tsify definition
        assert!(JsString::from_str("owner").unwrap().js_in(&js_order));
        assert!(JsString::from_str("evaluable").unwrap().js_in(&js_order));
        assert!(JsString::from_str("validInputs").unwrap().js_in(&js_order));
        assert!(JsString::from_str("validOutputs").unwrap().js_in(&js_order));
        assert!(JsString::from_str("nonce").unwrap().js_in(&js_order));
    }

    #[wasm_bindgen_test]
    fn test_evaluablev4_tsify() {
        let js_evaluable = to_js_value(&EvaluableV4::default()).unwrap();
        // validate serialized props match the tsify definition
        assert!(JsString::from_str("interpreter")
            .unwrap()
            .js_in(&js_evaluable));
        assert!(JsString::from_str("store").unwrap().js_in(&js_evaluable));
        assert!(JsString::from_str("bytecode").unwrap().js_in(&js_evaluable));
    }

    #[wasm_bindgen_test]
    fn test_signed_contextv2_tsify() {
        let js_signed_context = to_js_value(&SignedContextV2::default()).unwrap();
        // validate serialized props match the tsify definition
        assert!(JsString::from_str("signer")
            .unwrap()
            .js_in(&js_signed_context));
        assert!(JsString::from_str("domainSeparator")
            .unwrap()
            .js_in(&js_signed_context));
        assert!(JsString::from_str("context")
            .unwrap()
            .js_in(&js_signed_context));
        assert!(JsString::from_str("signature")
            .unwrap()
            .js_in(&js_signed_context));
    }

    #[wasm_bindgen_test]
    fn test_take_order_config_v4_tsify() {
        let js_take_order_config = to_js_value(&TakeOrderConfigV5::default()).unwrap();
        // validate serialized props match the tsify definition
        assert!(JsString::from_str("order")
            .unwrap()
            .js_in(&js_take_order_config));
        assert!(JsString::from_str("inputIOIndex")
            .unwrap()
            .js_in(&js_take_order_config));
        assert!(JsString::from_str("outputIOIndex")
            .unwrap()
            .js_in(&js_take_order_config));
        assert!(JsString::from_str("signedContext")
            .unwrap()
            .js_in(&js_take_order_config));
    }

    #[wasm_bindgen_test]
    fn test_take_orders_config_v4_tsify() {
        let js_take_orders_config = to_js_value(&TakeOrdersConfigV6::default()).unwrap();
        // validate serialized props match the tsify definition
        assert!(JsString::from_str("minimumIO")
            .unwrap()
            .js_in(&js_take_orders_config));
        assert!(JsString::from_str("maximumIO")
            .unwrap()
            .js_in(&js_take_orders_config));
        assert!(JsString::from_str("maximumIORatio")
            .unwrap()
            .js_in(&js_take_orders_config));
        assert!(JsString::from_str("IOIsInput")
            .unwrap()
            .js_in(&js_take_orders_config));
        assert!(JsString::from_str("orders")
            .unwrap()
            .js_in(&js_take_orders_config));
        assert!(JsString::from_str("data")
            .unwrap()
            .js_in(&js_take_orders_config));
    }
}
