use crate::raindex_client::RaindexError;
use crate::take_orders::{build_approval_calldata, BuiltTakeOrdersConfig, ParsedTakeOrdersMode};
use alloy::primitives::{Address, Bytes};
use alloy::sol_types::SolCall;
use rain_math_float::Float;
use raindex_bindings::IRaindexV7::takeOrders5Call;
use serde::{Deserialize, Serialize};
use std::ops::{Div, Mul};
use wasm_bindgen_utils::prelude::*;

#[derive(Clone)]
pub(crate) struct ApprovalInfoData {
    pub token: Address,
    pub spender: Address,
    pub amount: Float,
    pub formatted_amount: String,
    pub calldata: Bytes,
}

#[derive(Clone)]
pub(crate) struct TakeOrdersInfoData {
    pub raindex: Address,
    pub calldata: Bytes,
    pub effective_price: Float,
    pub prices: Vec<Float>,
    pub expected_sell: Float,
    pub max_sell_cap: Float,
}

#[derive(Clone)]
pub(crate) enum TakeOrdersCalldataResultInner {
    NeedsApproval(ApprovalInfoData),
    Ready(TakeOrdersInfoData),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct ApprovalInfo {
    token: Address,
    spender: Address,
    amount: Float,
    formatted_amount: String,
    calldata: Bytes,
}

impl ApprovalInfo {
    pub(crate) fn new(
        token: Address,
        spender: Address,
        amount: Float,
        formatted_amount: String,
        calldata: Bytes,
    ) -> Self {
        Self {
            token,
            spender,
            amount,
            formatted_amount,
            calldata,
        }
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl ApprovalInfo {
    #[wasm_bindgen(getter, unchecked_return_type = "Hex")]
    pub fn token(&self) -> String {
        self.token.to_string()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Hex")]
    pub fn spender(&self) -> String {
        self.spender.to_string()
    }
    #[wasm_bindgen(getter)]
    pub fn amount(&self) -> Float {
        self.amount
    }
    #[wasm_bindgen(getter = formattedAmount)]
    pub fn formatted_amount(&self) -> String {
        self.formatted_amount.clone()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Hex")]
    pub fn calldata(&self) -> String {
        self.calldata.to_string()
    }
}

#[cfg(not(target_family = "wasm"))]
impl ApprovalInfo {
    pub fn token(&self) -> Address {
        self.token
    }
    pub fn spender(&self) -> Address {
        self.spender
    }
    pub fn amount(&self) -> Float {
        self.amount
    }
    pub fn formatted_amount(&self) -> &str {
        &self.formatted_amount
    }
    pub fn calldata(&self) -> &Bytes {
        &self.calldata
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct TakeOrdersInfo {
    raindex: Address,
    calldata: Bytes,
    effective_price: Float,
    prices: Vec<Float>,
    expected_sell: Float,
    max_sell_cap: Float,
}

impl TakeOrdersInfo {
    pub(crate) fn new(
        raindex: Address,
        calldata: Bytes,
        effective_price: Float,
        prices: Vec<Float>,
        expected_sell: Float,
        max_sell_cap: Float,
    ) -> Self {
        Self {
            raindex,
            calldata,
            effective_price,
            prices,
            expected_sell,
            max_sell_cap,
        }
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl TakeOrdersInfo {
    #[wasm_bindgen(getter, unchecked_return_type = "Hex")]
    pub fn raindex(&self) -> String {
        self.raindex.to_string()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "Hex")]
    pub fn calldata(&self) -> String {
        self.calldata.to_string()
    }
    #[wasm_bindgen(getter = effectivePrice)]
    pub fn effective_price(&self) -> Float {
        self.effective_price
    }
    #[wasm_bindgen(getter)]
    pub fn prices(&self) -> Vec<Float> {
        self.prices.clone()
    }
    #[wasm_bindgen(getter = expectedSell)]
    pub fn expected_sell(&self) -> Float {
        self.expected_sell
    }
    #[wasm_bindgen(getter = maxSellCap)]
    pub fn max_sell_cap(&self) -> Float {
        self.max_sell_cap
    }
}

#[cfg(not(target_family = "wasm"))]
impl TakeOrdersInfo {
    pub fn raindex(&self) -> Address {
        self.raindex
    }
    pub fn calldata(&self) -> &Bytes {
        &self.calldata
    }
    pub fn effective_price(&self) -> Float {
        self.effective_price
    }
    pub fn prices(&self) -> &Vec<Float> {
        &self.prices
    }
    pub fn expected_sell(&self) -> Float {
        self.expected_sell
    }
    pub fn max_sell_cap(&self) -> Float {
        self.max_sell_cap
    }
}

#[wasm_bindgen]
pub struct TakeOrdersCalldataResult {
    inner: TakeOrdersCalldataResultInner,
}

impl std::fmt::Debug for TakeOrdersCalldataResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            TakeOrdersCalldataResultInner::NeedsApproval(data) => f
                .debug_struct("TakeOrdersCalldataResult::NeedsApproval")
                .field("token", &data.token)
                .field("spender", &data.spender)
                .finish(),
            TakeOrdersCalldataResultInner::Ready(data) => f
                .debug_struct("TakeOrdersCalldataResult::Ready")
                .field("raindex", &data.raindex)
                .finish(),
        }
    }
}

impl TakeOrdersCalldataResult {
    pub(crate) fn needs_approval(info: ApprovalInfoData) -> Self {
        Self {
            inner: TakeOrdersCalldataResultInner::NeedsApproval(info),
        }
    }

    pub(crate) fn ready(info: TakeOrdersInfoData) -> Self {
        Self {
            inner: TakeOrdersCalldataResultInner::Ready(info),
        }
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl TakeOrdersCalldataResult {
    #[wasm_bindgen(getter = isNeedsApproval)]
    pub fn is_needs_approval(&self) -> bool {
        matches!(self.inner, TakeOrdersCalldataResultInner::NeedsApproval(_))
    }

    #[wasm_bindgen(getter = isReady)]
    pub fn is_ready(&self) -> bool {
        matches!(self.inner, TakeOrdersCalldataResultInner::Ready(_))
    }

    #[wasm_bindgen(getter = approvalInfo)]
    pub fn approval_info(&self) -> Option<ApprovalInfo> {
        match &self.inner {
            TakeOrdersCalldataResultInner::NeedsApproval(data) => Some(ApprovalInfo::new(
                data.token,
                data.spender,
                data.amount,
                data.formatted_amount.clone(),
                data.calldata.clone(),
            )),
            TakeOrdersCalldataResultInner::Ready(_) => None,
        }
    }

    #[wasm_bindgen(getter = takeOrdersInfo)]
    pub fn take_orders_info(&self) -> Option<TakeOrdersInfo> {
        match &self.inner {
            TakeOrdersCalldataResultInner::NeedsApproval(_) => None,
            TakeOrdersCalldataResultInner::Ready(data) => Some(TakeOrdersInfo::new(
                data.raindex,
                data.calldata.clone(),
                data.effective_price,
                data.prices.clone(),
                data.expected_sell,
                data.max_sell_cap,
            )),
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl TakeOrdersCalldataResult {
    pub fn is_needs_approval(&self) -> bool {
        matches!(self.inner, TakeOrdersCalldataResultInner::NeedsApproval(_))
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.inner, TakeOrdersCalldataResultInner::Ready(_))
    }

    pub fn approval_info(&self) -> Option<ApprovalInfo> {
        match &self.inner {
            TakeOrdersCalldataResultInner::NeedsApproval(data) => Some(ApprovalInfo::new(
                data.token,
                data.spender,
                data.amount,
                data.formatted_amount.clone(),
                data.calldata.clone(),
            )),
            TakeOrdersCalldataResultInner::Ready(_) => None,
        }
    }

    pub fn take_orders_info(&self) -> Option<TakeOrdersInfo> {
        match &self.inner {
            TakeOrdersCalldataResultInner::NeedsApproval(_) => None,
            TakeOrdersCalldataResultInner::Ready(data) => Some(TakeOrdersInfo::new(
                data.raindex,
                data.calldata.clone(),
                data.effective_price,
                data.prices.clone(),
                data.expected_sell,
                data.max_sell_cap,
            )),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct TakeOrderEstimate {
    expected_spend: Float,
    expected_receive: Float,
    is_partial: bool,
}

impl TakeOrderEstimate {
    pub(crate) fn new(expected_spend: Float, expected_receive: Float, is_partial: bool) -> Self {
        Self {
            expected_spend,
            expected_receive,
            is_partial,
        }
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl TakeOrderEstimate {
    #[wasm_bindgen(getter = expectedSpend)]
    pub fn expected_spend(&self) -> Float {
        self.expected_spend
    }
    #[wasm_bindgen(getter = expectedReceive)]
    pub fn expected_receive(&self) -> Float {
        self.expected_receive
    }
    #[wasm_bindgen(getter = isPartial)]
    pub fn is_partial(&self) -> bool {
        self.is_partial
    }
}

#[cfg(not(target_family = "wasm"))]
impl TakeOrderEstimate {
    pub fn expected_spend(&self) -> Float {
        self.expected_spend
    }
    pub fn expected_receive(&self) -> Float {
        self.expected_receive
    }
    pub fn is_partial(&self) -> bool {
        self.is_partial
    }
}

pub(crate) fn build_approval_result(
    token: Address,
    spender: Address,
    amount: Float,
    decimals: u8,
) -> Result<TakeOrdersCalldataResult, RaindexError> {
    let amount_u256 = amount.to_fixed_decimal_lossy(decimals)?.0;
    let calldata = build_approval_calldata(spender, amount_u256);
    let truncated_amount = Float::from_fixed_decimal(amount_u256, decimals)?;
    let formatted_amount = truncated_amount.format().unwrap_or_default();
    Ok(TakeOrdersCalldataResult::needs_approval(ApprovalInfoData {
        token,
        spender,
        amount: truncated_amount,
        formatted_amount,
        calldata,
    }))
}

pub(crate) fn build_calldata_result(
    raindex_addr: Address,
    built_config: BuiltTakeOrdersConfig,
    mode: ParsedTakeOrdersMode,
    price_cap: Float,
) -> Result<TakeOrdersCalldataResult, RaindexError> {
    let calldata_bytes = takeOrders5Call {
        config: built_config.config,
    }
    .abi_encode();
    let calldata = Bytes::copy_from_slice(&calldata_bytes);

    let zero = Float::zero()?;
    let effective_price = if built_config.sim.total_output.gt(zero)? {
        built_config
            .sim
            .total_input
            .div(built_config.sim.total_output)?
    } else {
        zero
    };

    let prices: Vec<Float> = built_config
        .sim
        .legs
        .iter()
        .map(|leg| leg.candidate.ratio)
        .collect();

    let expected_sell = built_config.sim.total_input;
    let max_sell_cap = if mode.is_buy_mode() {
        mode.target_amount().mul(price_cap)?
    } else {
        mode.target_amount()
    };

    Ok(TakeOrdersCalldataResult::ready(TakeOrdersInfoData {
        raindex: raindex_addr,
        calldata,
        effective_price,
        prices,
        expected_sell,
        max_sell_cap,
    }))
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use crate::raindex_client::take_orders::selection::select_best_raindex_simulation;
    use crate::take_orders::build_take_orders_config_from_simulation;
    use crate::test_helpers::candidates::make_candidate;
    use alloy::primitives::U256;
    use raindex_bindings::IRaindexV7::takeOrders5Call;
    use raindex_bindings::IERC20::approveCall;

    fn high_price_cap() -> Float {
        Float::parse("1000000".to_string()).unwrap()
    }

    fn buy_up_to(amount: Float) -> ParsedTakeOrdersMode {
        ParsedTakeOrdersMode {
            mode: crate::take_orders::TakeOrdersMode::BuyUpTo,
            amount,
        }
    }

    fn spend_up_to(amount: Float) -> ParsedTakeOrdersMode {
        ParsedTakeOrdersMode {
            mode: crate::take_orders::TakeOrdersMode::SpendUpTo,
            amount,
        }
    }

    #[test]
    fn test_build_calldata_result_produces_valid_calldata_buy_mode() {
        let raindex_addr = Address::from([0x11u8; 20]);
        let max_output = Float::parse("10".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_candidate(raindex_addr, max_output, ratio);
        let candidates = vec![candidate];
        let buy_target = Float::parse("10".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = buy_up_to(buy_target);

        let (_, sim) = select_best_raindex_simulation(candidates, mode, price_cap).unwrap();
        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        let result = build_calldata_result(raindex_addr, built, mode, price_cap);

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_ready());
        let take_orders_info = result.take_orders_info().unwrap();
        assert_eq!(take_orders_info.raindex(), raindex_addr);
        assert!(!take_orders_info.calldata().is_empty());
        assert!(!take_orders_info.prices().is_empty());

        let decoded = takeOrders5Call::abi_decode(take_orders_info.calldata())
            .expect("Should decode calldata");
        assert!(!decoded.config.orders.is_empty());
        assert!(
            decoded.config.IOIsInput,
            "IOIsInput should be true for buy mode (taker output constraints)"
        );
    }

    #[test]
    fn test_build_calldata_result_produces_valid_calldata_spend_mode() {
        let raindex_addr = Address::from([0x11u8; 20]);
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_candidate(raindex_addr, max_output, ratio);
        let candidates = vec![candidate];
        let spend_budget = Float::parse("20".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = spend_up_to(spend_budget);

        let (_, sim) = select_best_raindex_simulation(candidates, mode, price_cap).unwrap();
        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        let result = build_calldata_result(raindex_addr, built, mode, price_cap);

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_ready());
        let take_orders_info = result.take_orders_info().unwrap();
        assert_eq!(take_orders_info.raindex(), raindex_addr);
        assert!(!take_orders_info.calldata().is_empty());
        assert!(!take_orders_info.prices().is_empty());

        let decoded = takeOrders5Call::abi_decode(take_orders_info.calldata())
            .expect("Should decode calldata");
        assert!(!decoded.config.orders.is_empty());
        assert!(
            !decoded.config.IOIsInput,
            "IOIsInput should be false for spend mode (taker input constraints)"
        );
    }

    #[test]
    fn test_build_calldata_result_effective_price_calculation() {
        let raindex_addr = Address::from([0x11u8; 20]);
        let max_output = Float::parse("10".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_candidate(raindex_addr, max_output, ratio);
        let candidates = vec![candidate];
        let buy_target = Float::parse("10".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = buy_up_to(buy_target);

        let (_, sim) = select_best_raindex_simulation(candidates, mode, price_cap).unwrap();
        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        let result = build_calldata_result(raindex_addr, built, mode, price_cap).unwrap();
        assert!(result.is_ready());
        let take_orders_info = result.take_orders_info().unwrap();

        let zero = Float::zero().unwrap();
        assert!(
            take_orders_info.effective_price().gt(zero).unwrap(),
            "Effective price should be > 0"
        );
    }

    #[test]
    fn test_build_calldata_result_prices_match_legs() {
        let raindex_addr = Address::from([0x11u8; 20]);
        let max_output = Float::parse("10".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_candidate(raindex_addr, max_output, ratio);
        let candidates = vec![candidate];
        let buy_target = Float::parse("10".to_string()).unwrap();
        let price_cap = high_price_cap();
        let mode = buy_up_to(buy_target);

        let (_, sim) = select_best_raindex_simulation(candidates, mode, price_cap).unwrap();
        let leg_count = sim.legs.len();
        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        let result = build_calldata_result(raindex_addr, built, mode, price_cap).unwrap();
        assert!(result.is_ready());
        let take_orders_info = result.take_orders_info().unwrap();

        assert_eq!(
            take_orders_info.prices().len(),
            leg_count,
            "Number of prices should match number of legs"
        );
        assert!(
            take_orders_info.prices()[0].eq(ratio).unwrap(),
            "Price should match the candidate ratio"
        );
    }

    #[test]
    fn test_build_calldata_result_expected_sell_and_max_sell_cap_buy_mode() {
        let raindex_addr = Address::from([0x11u8; 20]);
        let max_output = Float::parse("10".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_candidate(raindex_addr, max_output, ratio);
        let candidates = vec![candidate];
        let buy_target = Float::parse("10".to_string()).unwrap();
        let price_cap = Float::parse("3".to_string()).unwrap();
        let mode = buy_up_to(buy_target);

        let (_, sim) = select_best_raindex_simulation(candidates, mode, price_cap).unwrap();
        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        let result = build_calldata_result(raindex_addr, built, mode, price_cap).unwrap();
        assert!(result.is_ready());
        let take_orders_info = result.take_orders_info().unwrap();

        let expected_sell = Float::parse("20".to_string()).unwrap();
        let expected_max_sell_cap = Float::parse("30".to_string()).unwrap();

        assert!(
            take_orders_info.expected_sell().eq(expected_sell).unwrap(),
            "expected_sell should be output * ratio = 10 * 2 = 20"
        );
        assert!(
            take_orders_info
                .max_sell_cap()
                .eq(expected_max_sell_cap)
                .unwrap(),
            "max_sell_cap should be buy_target * price_cap = 10 * 3 = 30"
        );
    }

    #[test]
    fn test_build_calldata_result_expected_sell_and_max_sell_cap_spend_mode() {
        let raindex_addr = Address::from([0x11u8; 20]);
        let max_output = Float::parse("100".to_string()).unwrap();
        let ratio = Float::parse("2".to_string()).unwrap();
        let candidate = make_candidate(raindex_addr, max_output, ratio);
        let candidates = vec![candidate];
        let spend_budget = Float::parse("20".to_string()).unwrap();
        let price_cap = Float::parse("3".to_string()).unwrap();
        let mode = spend_up_to(spend_budget);

        let (_, sim) = select_best_raindex_simulation(candidates, mode, price_cap).unwrap();
        let built = build_take_orders_config_from_simulation(sim, mode, price_cap)
            .unwrap()
            .unwrap();

        let result = build_calldata_result(raindex_addr, built, mode, price_cap).unwrap();
        assert!(result.is_ready());
        let take_orders_info = result.take_orders_info().unwrap();

        let expected_sell = Float::parse("20".to_string()).unwrap();
        let expected_max_sell_cap = Float::parse("20".to_string()).unwrap();

        assert!(
            take_orders_info.expected_sell().eq(expected_sell).unwrap(),
            "expected_sell should equal spend_budget = 20"
        );
        assert!(
            take_orders_info
                .max_sell_cap()
                .eq(expected_max_sell_cap)
                .unwrap(),
            "max_sell_cap in spend mode should equal spend_budget = 20"
        );
    }

    #[test]
    fn test_build_approval_result_excess_decimal_precision() {
        let token = Address::from([0x22u8; 20]);
        let spender = Address::from([0x33u8; 20]);
        let amount = Float::parse("22.446685714285714".to_string()).unwrap();
        let decimals = 6u8;

        let result = build_approval_result(token, spender, amount, decimals);
        assert!(
            result.is_ok(),
            "Should not error on excess decimal precision: {:?}",
            result.err()
        );
        let result = result.unwrap();
        assert!(result.is_needs_approval());
        let approval_info = result.approval_info().unwrap();
        assert_eq!(approval_info.token(), token);
        assert_eq!(approval_info.spender(), spender);
        assert!(!approval_info.calldata().is_empty());

        let decoded = approveCall::abi_decode(approval_info.calldata())
            .expect("Should decode approval calldata");
        let expected_truncated = U256::from(22_446_685u64);
        assert_eq!(
            decoded.amount, expected_truncated,
            "Approved amount should be 22.446685 truncated to 6 decimals = 22446685"
        );

        let truncated_float = Float::parse("22.446685".to_string()).unwrap();
        assert!(
            approval_info.amount().eq(truncated_float).unwrap(),
            "Displayed amount should match truncated value, not original"
        );
    }

    #[test]
    fn test_build_approval_result_produces_valid_approval_info() {
        let token = Address::from([0x22u8; 20]);
        let spender = Address::from([0x33u8; 20]);
        let amount = Float::parse("1000".to_string()).unwrap();
        let decimals = 18u8;

        let result = build_approval_result(token, spender, amount, decimals).unwrap();

        assert!(result.is_needs_approval());
        let approval_info = result.approval_info().unwrap();
        assert_eq!(approval_info.token(), token);
        assert_eq!(approval_info.spender(), spender);
        assert!(approval_info.amount().eq(amount).unwrap());
        assert_eq!(approval_info.formatted_amount(), "1000");
        assert!(!approval_info.calldata().is_empty());
    }
}
