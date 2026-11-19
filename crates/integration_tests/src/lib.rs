#[cfg(test)]
mod tests {
    use alloy::primitives::{Address, Bytes, B256};
    use alloy::serde::WithOtherFields;
    use alloy::sol_types::SolCall;
    use alloy::{
        network::TransactionBuilder,
        primitives::{utils::parse_ether, U256},
        rpc::types::TransactionRequest,
    };
    use rain_math_float::Float;
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_common::{add_order::AddOrderArgs, dotrain_order::DotrainOrder};
    use raindex_test_fixtures::{
        LocalEvm,
        Raindex::{self, ClearConfigV2, OrderV4, QuoteV3, TakeOrderConfigV5, TakeOrdersConfigV6},
    };

    #[tokio::test]
    async fn test_post_task_set() {
        let local_evm = LocalEvm::new_with_tokens(2).await;

        let raindex = &local_evm.raindex;

        let token1_holder = local_evm.signer_wallets[0].default_signer().address();

        let token1 = local_evm.tokens[0].clone();
        let token2 = local_evm.tokens[1].clone();

        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    some-key:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    some-key:
        address: {rainlang_address}
tokens:
    eth:
        network: some-key
        address: {token2}
        decimals: 18
        label: Ethereum
        symbol: ETH
    dai:
        network: some-key
        address: {token1}
        decimals: 18
        label: Dai
        symbol: DAI
raindex:
    some-key:
        address: {raindex}
orders:
    some-key:
        inputs:
            - token: eth
        outputs:
            - token: dai
              vault-id: 0x01
scenarios:
    some-key:
        rainlang: some-key
        bindings:
            key1: 10
deployments:
    some-key:
        scenario: some-key
        order: some-key
---
#key1 !Test binding
#calculate-io
using-words-from {raindex_subparser}
amount price: get("amount") 52;
#handle-add-order
:set("amount" 100);
#handle-io
:;
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            raindex_subparser = local_evm.raindex_subparser.address(),
            rainlang_address = local_evm.rainlang,
            token1 = token1.address(),
            token2 = token2.address(),
            spec_version = SpecVersion::current()
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .try_into_call(vec![local_evm.url()])
            .await
            .unwrap()
            .abi_encode();

        let order = local_evm
            .add_order_and_deposit(
                &calldata,
                token1_holder,
                *token1.address(),
                parse_ether("1000").unwrap(),
                18,
                B256::from(U256::from(1)),
            )
            .await
            .0
            .order;

        let quote = local_evm
            .call_contract(raindex.quote3(QuoteV3 {
                order,
                inputIOIndex: U256::from(0),
                outputIOIndex: U256::from(0),
                signedContext: vec![],
            }))
            .await
            .unwrap()
            .unwrap();

        let amount = Float::from_raw(quote._1);
        let price = Float::from_raw(quote._2);

        let expected_amount = Float::parse("100".to_string()).unwrap();
        let expected_price = Float::parse("52".to_string()).unwrap();

        assert!(amount.eq(expected_amount).unwrap());
        assert!(price.eq(expected_price).unwrap());
    }

    #[tokio::test]
    async fn test_post_task_revert() {
        let local_evm = LocalEvm::new().await;
        let raindex = &local_evm.raindex;

        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    some-key:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    some-key:
        address: {rainlang_address}
tokens:
    eth:
        network: some-key
        address: 0xabc0000000000000000000000000000000000003
        decimals: 18
        label: Ethereum
        symbol: ETH
    dai:
        network: some-key
        address: 0xabc0000000000000000000000000000000000004
        decimals: 18
        label: Dai
        symbol: DAI
raindex:
    some-key:
        address: {raindex}
orders:
    some-key:
        inputs:
            - token: eth
            - token: dai
        outputs:
            - token: dai
scenarios:
    some-key:
        rainlang: some-key
        bindings:
            key1: 10
deployments:
    some-key:
        scenario: some-key
        order: some-key
---
#key1 !Test binding
#calculate-io
amount price: get("amount") 52;
#handle-io
:;
#handle-add-order
:ensure(0 "should fail");
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            rainlang_address = local_evm.rainlang,
            spec_version = SpecVersion::current(),
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .try_into_call(vec![local_evm.url()])
            .await
            .unwrap()
            .abi_encode();
        let tx = TransactionRequest::default()
            .with_input(calldata)
            .with_to(*raindex.address());

        let res = local_evm
            .send_transaction(WithOtherFields::new(tx))
            .await
            .expect_err("Transaction should have reverted");

        assert!(res.to_string().contains("should fail"));
    }

    /// A minimal order that sells `dai` (`token1`, the output vault) for `eth`
    /// (`token2`) at a fixed `outputMax = 2`, `IORatio = 1`. The output vault is
    /// funded with 10 `dai` so the order is fully takeable when live. The owner
    /// is `from`, which lets us add a second order from a distinct owner for the
    /// `clear4` two-party path.
    ///
    /// Returns the on-chain `OrderV4` and its hash (the `AddOrderV3` event's
    /// `orderHash`, which is exactly what the contract keys `sOrders` on, hence
    /// what `OrderNotFound` reports).
    async fn add_takeable_order(
        local_evm: &LocalEvm,
        from: Address,
        token1: &Address,
        token2: &Address,
        vault_id: B256,
    ) -> (OrderV4, B256) {
        let raindex = &local_evm.raindex;

        let dotrain = format!(
            r#"
version: {spec_version}
networks:
    some-key:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    some-key:
        address: {rainlang_address}
tokens:
    eth:
        network: some-key
        address: {token2}
        decimals: 18
        label: Ethereum
        symbol: ETH
    dai:
        network: some-key
        address: {token1}
        decimals: 18
        label: Dai
        symbol: DAI
raindex:
    some-key:
        address: {raindex}
orders:
    some-key:
        inputs:
            - token: eth
        outputs:
            - token: dai
              vault-id: {vault_id}
scenarios:
    some-key:
        rainlang: some-key
        bindings:
            key1: 10
deployments:
    some-key:
        scenario: some-key
        order: some-key
---
#key1 !Test binding
#calculate-io
amount price: 2 1;
#handle-add-order
:;
#handle-io
:;
"#,
            rpc_url = local_evm.url(),
            raindex = raindex.address(),
            rainlang_address = local_evm.rainlang,
            token1 = token1,
            token2 = token2,
            vault_id = vault_id,
            spec_version = SpecVersion::current(),
        );

        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .try_into_call(vec![local_evm.url()])
            .await
            .unwrap()
            .abi_encode();

        let (event, ..) = local_evm
            .add_order_and_deposit(
                &calldata,
                from,
                *token1,
                parse_ether("10").unwrap(),
                18,
                vault_id,
            )
            .await;

        (event.order, event.orderHash)
    }

    /// Counts the `OrderNotFound`, `TakeOrderV4`, `ClearV3` and `AfterClearV2`
    /// logs emitted by a single transaction receipt. These are the events that
    /// discriminate the best-effort no-op behaviour: a dead order is announced
    /// with `OrderNotFound` and contributes no settlement event.
    fn count_outcome_events(
        receipt: &alloy::network::AnyTransactionReceipt,
    ) -> (usize, usize, usize, usize) {
        let logs = receipt.inner.logs();
        let order_not_found = logs
            .iter()
            .filter(|l| l.log_decode::<Raindex::OrderNotFound>().is_ok())
            .count();
        let take_order = logs
            .iter()
            .filter(|l| l.log_decode::<Raindex::TakeOrderV4>().is_ok())
            .count();
        let clear = logs
            .iter()
            .filter(|l| l.log_decode::<Raindex::ClearV3>().is_ok())
            .count();
        let after_clear = logs
            .iter()
            .filter(|l| l.log_decode::<Raindex::AfterClearV2>().is_ok())
            .count();
        (order_not_found, take_order, clear, after_clear)
    }

    /// Decodes the single `OrderNotFound` event from a receipt, asserting there
    /// is exactly one.
    fn expect_single_order_not_found(
        receipt: &alloy::network::AnyTransactionReceipt,
    ) -> Raindex::OrderNotFound {
        let mut found: Vec<Raindex::OrderNotFound> = receipt
            .inner
            .logs()
            .iter()
            .filter_map(|l| l.log_decode::<Raindex::OrderNotFound>().ok())
            .map(|l| l.inner.data)
            .collect();
        assert_eq!(found.len(), 1, "expected exactly one OrderNotFound event");
        found.remove(0)
    }

    /// `takeOrders5` against a single dead order is a best-effort no-op: the
    /// transaction succeeds, the contract emits `OrderNotFound` (caller, owner,
    /// orderHash) instead of reverting, no `TakeOrderV4` is emitted, and the
    /// returned `(totalTakerInput, totalTakerOutput)` are both zero.
    #[tokio::test]
    async fn test_take_orders_dead_order_is_noop() {
        let local_evm = LocalEvm::new_with_tokens(2).await;
        let raindex = &local_evm.raindex;

        let owner = local_evm.signer_wallets[0].default_signer().address();
        let taker = local_evm.signer_wallets[1].default_signer().address();
        let token1 = *local_evm.tokens[0].address();
        let token2 = *local_evm.tokens[1].address();

        let (order, order_hash) = add_takeable_order(
            &local_evm,
            owner,
            &token1,
            &token2,
            B256::from(U256::from(1)),
        )
        .await;

        // Kill the order. After this `sOrders[orderHash] == ORDER_DEAD`.
        local_evm
            .send_transaction(
                raindex
                    .removeOrder4(order.clone(), vec![])
                    .from(owner)
                    .into_transaction_request(),
            )
            .await
            .expect("removeOrder4 should succeed");

        assert!(
            !raindex
                .orderExists(order_hash)
                .call()
                .await
                .expect("orderExists call should succeed"),
            "order must be dead before takeOrders5"
        );

        let max_io = Float::parse("1000".to_string()).unwrap().get_inner();
        let config = TakeOrdersConfigV6 {
            minimumIO: Float::parse("0".to_string()).unwrap().get_inner(),
            maximumIO: max_io,
            maximumIORatio: max_io,
            IOIsInput: true,
            orders: vec![TakeOrderConfigV5 {
                order: order.clone(),
                inputIOIndex: U256::from(0),
                outputIOIndex: U256::from(0),
                signedContext: vec![],
            }],
            data: Bytes::new(),
        };

        // Static call to read the returned totals: both must be zero.
        let returns = local_evm
            .call_contract(raindex.takeOrders5(config.clone()).from(taker))
            .await
            .expect("rpc should succeed")
            .expect("takeOrders5 should not revert against a dead order");
        let total_input = Float::from_raw(returns.totalTakerInput);
        let total_output = Float::from_raw(returns.totalTakerOutput);
        assert!(
            total_input
                .eq(Float::parse("0".to_string()).unwrap())
                .unwrap(),
            "totalTakerInput must be zero for a dead order"
        );
        assert!(
            total_output
                .eq(Float::parse("0".to_string()).unwrap())
                .unwrap(),
            "totalTakerOutput must be zero for a dead order"
        );

        // Now actually mine it and assert the emitted events.
        let receipt = local_evm
            .send_transaction(
                raindex
                    .takeOrders5(config)
                    .from(taker)
                    .into_transaction_request(),
            )
            .await
            .expect("takeOrders5 against a dead order must succeed, not revert");

        let (order_not_found, take_order, _clear, _after_clear) = count_outcome_events(&receipt);
        assert_eq!(order_not_found, 1, "exactly one OrderNotFound expected");
        assert_eq!(
            take_order, 0,
            "no TakeOrderV4 must be emitted for a dead order"
        );

        let event = expect_single_order_not_found(&receipt);
        assert_eq!(
            event.sender, taker,
            "OrderNotFound.sender must be the caller"
        );
        assert_eq!(
            event.owner, owner,
            "OrderNotFound.owner must be the order owner"
        );
        assert_eq!(
            event.orderHash, order_hash,
            "OrderNotFound.orderHash must match"
        );
    }

    /// In a `takeOrders5` batch of [dead, live] orders the dead order is skipped
    /// (best-effort) while the live order is still taken: the receipt carries
    /// exactly one `OrderNotFound` (for the dead order) AND exactly one
    /// `TakeOrderV4` (for the live order), and the returned totals reflect the
    /// live fill only (taker buys the live order's full `outputMax = 2`).
    #[tokio::test]
    async fn test_take_orders_dead_order_skipped_live_order_taken() {
        let local_evm = LocalEvm::new_with_tokens(2).await;
        let raindex = &local_evm.raindex;

        let owner = local_evm.signer_wallets[0].default_signer().address();
        let taker = local_evm.signer_wallets[0].default_signer().address();
        let token1 = *local_evm.tokens[0].address();
        let token2 = *local_evm.tokens[1].address();

        // Dead order in vault 1.
        let (dead_order, dead_hash) = add_takeable_order(
            &local_evm,
            owner,
            &token1,
            &token2,
            B256::from(U256::from(1)),
        )
        .await;
        // Live order in vault 2 (so its deposit is independent).
        let (live_order, _live_hash) = add_takeable_order(
            &local_evm,
            owner,
            &token1,
            &token2,
            B256::from(U256::from(2)),
        )
        .await;

        local_evm
            .send_transaction(
                raindex
                    .removeOrder4(dead_order.clone(), vec![])
                    .from(owner)
                    .into_transaction_request(),
            )
            .await
            .expect("removeOrder4 should succeed");

        // The taker pays `eth` (token2); approve raindex to pull it.
        local_evm
            .send_transaction(
                local_evm.tokens[1]
                    .approve(*raindex.address(), parse_ether("1000").unwrap())
                    .from(taker)
                    .into_transaction_request(),
            )
            .await
            .expect("approve should succeed");

        let max_io = Float::parse("1000".to_string()).unwrap().get_inner();
        let config = TakeOrdersConfigV6 {
            minimumIO: Float::parse("0".to_string()).unwrap().get_inner(),
            maximumIO: max_io,
            maximumIORatio: max_io,
            IOIsInput: true,
            orders: vec![
                TakeOrderConfigV5 {
                    order: dead_order,
                    inputIOIndex: U256::from(0),
                    outputIOIndex: U256::from(0),
                    signedContext: vec![],
                },
                TakeOrderConfigV5 {
                    order: live_order,
                    inputIOIndex: U256::from(0),
                    outputIOIndex: U256::from(0),
                    signedContext: vec![],
                },
            ],
            data: Bytes::new(),
        };

        let receipt = local_evm
            .send_transaction(
                raindex
                    .takeOrders5(config)
                    .from(taker)
                    .into_transaction_request(),
            )
            .await
            .expect("mixed dead/live takeOrders5 must succeed");

        let (order_not_found, take_order, _clear, _after_clear) = count_outcome_events(&receipt);
        assert_eq!(
            order_not_found, 1,
            "the dead order must produce exactly one OrderNotFound"
        );
        assert_eq!(
            take_order, 1,
            "the live order must still be taken (one TakeOrderV4)"
        );

        // The dead order is the one announced, not the live one.
        let event = expect_single_order_not_found(&receipt);
        assert_eq!(
            event.orderHash, dead_hash,
            "OrderNotFound must name the dead order"
        );

        // The live order's full outputMax (2 dai) is bought at ratio 1, so the
        // taker receives 2 dai (input) for 2 eth (output).
        let taken = receipt
            .inner
            .logs()
            .iter()
            .find_map(|l| l.log_decode::<Raindex::TakeOrderV4>().ok())
            .expect("a TakeOrderV4 must be present")
            .inner
            .data;
        let taker_input = Float::from_raw(taken.input);
        let taker_output = Float::from_raw(taken.output);
        assert!(
            taker_input
                .eq(Float::parse("2".to_string()).unwrap())
                .unwrap(),
            "taker input should be the live order's full 2 dai"
        );
        assert!(
            taker_output
                .eq(Float::parse("2".to_string()).unwrap())
                .unwrap(),
            "taker output should be 2 eth at ratio 1"
        );
    }

    /// `clear4` where one side is a dead order is a best-effort no-op: the
    /// transaction succeeds, the contract emits `OrderNotFound` for the dead
    /// side and returns early, so neither `ClearV3` nor `AfterClearV2` is
    /// emitted. This mirrors the on-chain early-return that makes bulk clearing
    /// via `Multicall` safe against stale/removed orders.
    #[tokio::test]
    async fn test_clear_dead_order_is_noop() {
        let local_evm = LocalEvm::new_with_tokens(2).await;
        let raindex = &local_evm.raindex;

        let alice = local_evm.signer_wallets[0].default_signer().address();
        let bob = local_evm.signer_wallets[1].default_signer().address();
        let token1 = *local_evm.tokens[0].address();
        let token2 = *local_evm.tokens[1].address();

        // Alice sells dai (token1) for eth (token2); her order is killed below.
        let (alice_order, alice_hash) = add_takeable_order(
            &local_evm,
            alice,
            &token1,
            &token2,
            B256::from(U256::from(1)),
        )
        .await;

        // Bob's order is the complementary side: he sells eth (token2) for dai
        // (token1). Built directly so the input/output token pair is the mirror
        // of Alice's, satisfying clear4's TokenMismatch check.
        let bob_order = OrderV4 {
            owner: bob,
            evaluable: alice_order.evaluable.clone(),
            validInputs: alice_order.validOutputs.clone(),
            validOutputs: alice_order.validInputs.clone(),
            nonce: B256::from(U256::from(0xb0b)),
        };

        // Kill Alice's order.
        local_evm
            .send_transaction(
                raindex
                    .removeOrder4(alice_order.clone(), vec![])
                    .from(alice)
                    .into_transaction_request(),
            )
            .await
            .expect("removeOrder4 should succeed");

        assert!(
            !raindex
                .orderExists(alice_hash)
                .call()
                .await
                .expect("orderExists call should succeed"),
            "alice's order must be dead before clear4"
        );

        let clear_config = ClearConfigV2 {
            aliceInputIOIndex: U256::from(0),
            aliceOutputIOIndex: U256::from(0),
            bobInputIOIndex: U256::from(0),
            bobOutputIOIndex: U256::from(0),
            aliceBountyVaultId: B256::from(U256::from(0)),
            bobBountyVaultId: B256::from(U256::from(0)),
        };

        let receipt = local_evm
            .send_transaction(
                raindex
                    .clear4(alice_order, bob_order, clear_config, vec![], vec![])
                    .from(bob)
                    .into_transaction_request(),
            )
            .await
            .expect("clear4 against a dead order must succeed, not revert");

        let (order_not_found, _take_order, clear, after_clear) = count_outcome_events(&receipt);
        assert_eq!(order_not_found, 1, "exactly one OrderNotFound expected");
        assert_eq!(clear, 0, "no ClearV3 must be emitted when a side is dead");
        assert_eq!(
            after_clear, 0,
            "no AfterClearV2 must be emitted when a side is dead"
        );

        let event = expect_single_order_not_found(&receipt);
        assert_eq!(
            event.sender, bob,
            "OrderNotFound.sender must be the clearer"
        );
        assert_eq!(
            event.owner, alice,
            "OrderNotFound.owner must be the dead order's owner"
        );
        assert_eq!(
            event.orderHash, alice_hash,
            "OrderNotFound.orderHash must match alice's"
        );
    }
}
