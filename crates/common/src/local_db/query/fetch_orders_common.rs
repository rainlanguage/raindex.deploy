use super::fetch_orders::FetchOrdersArgs;
use crate::local_db::query::{SqlBuildError, SqlStatement, SqlValue};
use alloy::primitives::{Address, U256};

use super::fetch_orders::FetchOrdersActiveFilter;

pub(crate) const OWNERS_CLAUSE: &str = "/*OWNERS_CLAUSE*/";
pub(crate) const OWNERS_CLAUSE_BODY: &str = "AND l.order_owner IN ({list})";

pub(crate) const ORDER_HASH_CLAUSE: &str = "/*ORDER_HASH_CLAUSE*/";
pub(crate) const ORDER_HASH_CLAUSE_BODY: &str =
    "AND COALESCE(la.order_hash, l.order_hash) = {param}";

pub(crate) const INPUT_TOKENS_CLAUSE: &str = "/*INPUT_TOKENS_CLAUSE*/";
pub(crate) const INPUT_TOKENS_CLAUSE_BODY: &str = "AND EXISTS (
      SELECT 1 FROM order_ios io2
      WHERE io2.chain_id = l.chain_id
        AND io2.raindex_address = l.raindex_address
        AND io2.transaction_hash = la.transaction_hash
        AND io2.log_index = la.log_index
        AND lower(io2.io_type) = 'input'
        AND io2.token IN ({list})
    )";

pub(crate) const OUTPUT_TOKENS_CLAUSE: &str = "/*OUTPUT_TOKENS_CLAUSE*/";
pub(crate) const OUTPUT_TOKENS_CLAUSE_BODY: &str = "AND EXISTS (
      SELECT 1 FROM order_ios io2
      WHERE io2.chain_id = l.chain_id
        AND io2.raindex_address = l.raindex_address
        AND io2.transaction_hash = la.transaction_hash
        AND io2.log_index = la.log_index
        AND lower(io2.io_type) = 'output'
        AND io2.token IN ({list})
    )";

pub(crate) const COMBINED_TOKENS_CLAUSE_BODY: &str = "AND EXISTS (
      SELECT 1 FROM order_ios io2
      WHERE io2.chain_id = l.chain_id
        AND io2.raindex_address = l.raindex_address
        AND io2.transaction_hash = la.transaction_hash
        AND io2.log_index = la.log_index
        AND (
          (lower(io2.io_type) = 'input' AND io2.token IN ({input_list}))
          OR
          (lower(io2.io_type) = 'output' AND io2.token IN ({output_list}))
        )
    )";

pub(crate) const POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE: &str =
    "/*POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE*/";
const POSITIVE_OUTPUT_VAULT_BALANCE_EXISTS_BODY: &str = "EXISTS (
      SELECT 1 FROM order_ios io_balance
      JOIN running_vault_balances vb_balance
        ON vb_balance.chain_id = io_balance.chain_id
       AND vb_balance.raindex_address = io_balance.raindex_address
       AND vb_balance.owner = l.order_owner
       AND vb_balance.token = io_balance.token
       AND vb_balance.vault_id = io_balance.vault_id
      WHERE io_balance.chain_id = l.chain_id
        AND io_balance.raindex_address = l.raindex_address
        AND io_balance.transaction_hash = la.transaction_hash
        AND io_balance.log_index = la.log_index
        AND lower(io_balance.io_type) = 'output'
        AND io_balance.vault_id != {zero_vault_id}
        AND FLOAT_GT_ZERO(vb_balance.balance)
    )";

pub(crate) const MAIN_CHAIN_IDS_CLAUSE: &str = "/*MAIN_CHAIN_IDS_CLAUSE*/";
pub(crate) const MAIN_CHAIN_IDS_CLAUSE_BODY: &str = "AND oe.chain_id IN ({list})";
pub(crate) const MAIN_RAINDEXES_CLAUSE: &str = "/*MAIN_RAINDEXES_CLAUSE*/";
pub(crate) const MAIN_RAINDEXES_CLAUSE_BODY: &str = "AND oe.raindex_address IN ({list})";

pub(crate) const LATEST_ADD_CHAIN_IDS_CLAUSE: &str = "/*LATEST_ADD_CHAIN_IDS_CLAUSE*/";
pub(crate) const LATEST_ADD_CHAIN_IDS_CLAUSE_BODY: &str = "AND oe.chain_id IN ({list})";
pub(crate) const LATEST_ADD_RAINDEXES_CLAUSE: &str = "/*LATEST_ADD_RAINDEXES_CLAUSE*/";
pub(crate) const LATEST_ADD_RAINDEXES_CLAUSE_BODY: &str = "AND oe.raindex_address IN ({list})";

pub(crate) struct PreparedFilters {
    pub chain_ids: Vec<u32>,
    pub raindexes: Vec<Address>,
}

pub(crate) fn bind_common_order_filters(
    stmt: &mut SqlStatement,
    args: &FetchOrdersArgs,
) -> Result<PreparedFilters, SqlBuildError> {
    let active_str = match args.filter {
        FetchOrdersActiveFilter::All => "all",
        FetchOrdersActiveFilter::Active => "active",
        FetchOrdersActiveFilter::Inactive => "inactive",
    };
    stmt.push(SqlValue::from(active_str));

    let mut chain_ids = args.chain_ids.clone();
    chain_ids.sort_unstable();
    chain_ids.dedup();

    let mut raindexes = args.raindex_addresses.clone();
    raindexes.sort();
    raindexes.dedup();

    let chain_ids_iter = || chain_ids.iter().cloned().map(SqlValue::from);
    let raindexes_iter = || raindexes.iter().cloned().map(SqlValue::from);

    stmt.bind_list_clause(
        MAIN_CHAIN_IDS_CLAUSE,
        MAIN_CHAIN_IDS_CLAUSE_BODY,
        chain_ids_iter(),
    )?;
    stmt.bind_list_clause(
        LATEST_ADD_CHAIN_IDS_CLAUSE,
        LATEST_ADD_CHAIN_IDS_CLAUSE_BODY,
        chain_ids_iter(),
    )?;

    stmt.bind_list_clause(
        MAIN_RAINDEXES_CLAUSE,
        MAIN_RAINDEXES_CLAUSE_BODY,
        raindexes_iter(),
    )?;
    stmt.bind_list_clause(
        LATEST_ADD_RAINDEXES_CLAUSE,
        LATEST_ADD_RAINDEXES_CLAUSE_BODY,
        raindexes_iter(),
    )?;

    let mut owners = args.owners.clone();
    owners.sort();
    owners.dedup();
    stmt.bind_list_clause(
        OWNERS_CLAUSE,
        OWNERS_CLAUSE_BODY,
        owners.into_iter().map(SqlValue::from),
    )?;

    let order_hash_val = args.order_hash.as_ref().map(|hash| SqlValue::from(*hash));
    stmt.bind_param_clause(ORDER_HASH_CLAUSE, ORDER_HASH_CLAUSE_BODY, order_hash_val)?;

    let mut input_tokens = args.tokens.inputs.clone();
    input_tokens.sort();
    input_tokens.dedup();

    let mut output_tokens = args.tokens.outputs.clone();
    output_tokens.sort();
    output_tokens.dedup();

    let has_inputs = !input_tokens.is_empty();
    let has_outputs = !output_tokens.is_empty();

    if has_inputs && has_outputs && input_tokens == output_tokens {
        let input_placeholders: Vec<String> = input_tokens
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", stmt.params.len() + i + 1))
            .collect();
        let input_list_str = input_placeholders.join(", ");

        for token in &input_tokens {
            stmt.push(SqlValue::from(*token));
        }

        let output_placeholders: Vec<String> = output_tokens
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", stmt.params.len() + i + 1))
            .collect();
        let output_list_str = output_placeholders.join(", ");

        for token in &output_tokens {
            stmt.push(SqlValue::from(*token));
        }

        let combined_clause = COMBINED_TOKENS_CLAUSE_BODY
            .replace("{input_list}", &input_list_str)
            .replace("{output_list}", &output_list_str);

        stmt.sql = stmt.sql.replace(INPUT_TOKENS_CLAUSE, &combined_clause);
        stmt.sql = stmt.sql.replace(OUTPUT_TOKENS_CLAUSE, "");
    } else {
        stmt.bind_list_clause(
            INPUT_TOKENS_CLAUSE,
            INPUT_TOKENS_CLAUSE_BODY,
            input_tokens.into_iter().map(SqlValue::from),
        )?;
        stmt.bind_list_clause(
            OUTPUT_TOKENS_CLAUSE,
            OUTPUT_TOKENS_CLAUSE_BODY,
            output_tokens.into_iter().map(SqlValue::from),
        )?;
    }

    if args.has_positive_output_vault_balance == Some(true) {
        let zero_vault_id = stmt.push(SqlValue::from(U256::ZERO));
        let exists =
            POSITIVE_OUTPUT_VAULT_BALANCE_EXISTS_BODY.replace("{zero_vault_id}", &zero_vault_id);
        let clause = format!("AND {exists}");
        stmt.replace(POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE, &clause)?;
    } else {
        stmt.replace(POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE, "")?;
    }

    Ok(PreparedFilters {
        chain_ids,
        raindexes,
    })
}

#[cfg(test)]
mod tests {
    use super::super::fetch_orders::{
        FetchOrdersActiveFilter, FetchOrdersArgs, FetchOrdersTokensFilter,
    };
    use super::*;
    use alloy::primitives::address;

    // Template carrying every marker the binder replaces, run in isolation.
    fn template() -> String {
        format!(
            "X ?1 {} {} {} {} {} {} {} {} {}",
            MAIN_CHAIN_IDS_CLAUSE,
            LATEST_ADD_CHAIN_IDS_CLAUSE,
            MAIN_RAINDEXES_CLAUSE,
            LATEST_ADD_RAINDEXES_CLAUSE,
            OWNERS_CLAUSE,
            ORDER_HASH_CLAUSE,
            INPUT_TOKENS_CLAUSE,
            OUTPUT_TOKENS_CLAUSE,
            POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE,
        )
    }

    fn bind(args: &FetchOrdersArgs) -> (SqlStatement, PreparedFilters) {
        let mut stmt = SqlStatement::new(template());
        let prepared = bind_common_order_filters(&mut stmt, args).unwrap();
        (stmt, prepared)
    }

    fn text_params(stmt: &SqlStatement) -> Vec<String> {
        stmt.params()
            .iter()
            .filter_map(|p| match p {
                SqlValue::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    // Each active-filter variant maps to its exact lowercase string, pushed as
    // the first parameter.
    #[test]
    fn active_filter_variants_map_to_exact_strings() {
        for (filter, expected) in [
            (FetchOrdersActiveFilter::All, "all"),
            (FetchOrdersActiveFilter::Active, "active"),
            (FetchOrdersActiveFilter::Inactive, "inactive"),
        ] {
            let args = FetchOrdersArgs {
                filter,
                ..FetchOrdersArgs::default()
            };
            let (stmt, _) = bind(&args);
            assert_eq!(stmt.params()[0], SqlValue::Text(expected.to_string()));
        }
    }

    // Owners are deduplicated and sorted before binding, so a duplicated and
    // unsorted input yields exactly the unique sorted set in the bound owner
    // params.
    #[test]
    fn owners_are_sorted_and_deduplicated() {
        let a = address!("0x00000000000000000000000000000000000000aa");
        let b = address!("0x00000000000000000000000000000000000000bb");
        let c = address!("0x00000000000000000000000000000000000000cc");
        let args = FetchOrdersArgs {
            owners: vec![c, a, b, a, c],
            ..FetchOrdersArgs::default()
        };
        let (stmt, _) = bind(&args);
        // params[0] = active filter ("all"); the next three are the sorted,
        // deduplicated owners.
        let owners: Vec<String> = text_params(&stmt)
            .into_iter()
            .filter(|s| s.starts_with("0x000000000000000000000000000000000000"))
            .collect();
        assert_eq!(
            owners,
            vec![
                "0x00000000000000000000000000000000000000aa".to_string(),
                "0x00000000000000000000000000000000000000bb".to_string(),
                "0x00000000000000000000000000000000000000cc".to_string(),
            ]
        );
        assert!(stmt.sql().contains("AND l.order_owner IN (?2, ?3, ?4)"));
    }

    // The returned chain_ids are the unique, ascending set (consumed by the
    // caller for the remaining clauses).
    #[test]
    fn prepared_chain_ids_are_sorted_and_deduplicated() {
        let args = FetchOrdersArgs {
            chain_ids: vec![137, 1, 137, 42, 1],
            ..FetchOrdersArgs::default()
        };
        let (_, prepared) = bind(&args);
        assert_eq!(prepared.chain_ids, vec![1, 42, 137]);
    }

    // The returned raindexes are the unique, sorted set.
    #[test]
    fn prepared_raindexes_are_sorted_and_deduplicated() {
        let a = address!("0x00000000000000000000000000000000000000aa");
        let b = address!("0x00000000000000000000000000000000000000bb");
        let args = FetchOrdersArgs {
            raindex_addresses: vec![b, a, b],
            ..FetchOrdersArgs::default()
        };
        let (_, prepared) = bind(&args);
        assert_eq!(prepared.raindexes, vec![a, b]);
    }

    // When input_tokens equals output_tokens, ONE EXISTS with OR-logic is
    // emitted (not two EXISTS), and the OUTPUT_TOKENS marker is removed.
    #[test]
    fn canonical_equal_input_output_tokens_use_single_combined_exists() {
        let a = address!("0x00000000000000000000000000000000000000aa");
        let b = address!("0x00000000000000000000000000000000000000bb");
        let args = FetchOrdersArgs {
            tokens: FetchOrdersTokensFilter {
                inputs: vec![b, a, a],
                outputs: vec![a, b, b],
            },
            ..FetchOrdersArgs::default()
        };
        let (stmt, _) = bind(&args);
        // Single combined EXISTS holding BOTH directional io_type checks joined
        // by OR (the multi-line `OR` from COMBINED_TOKENS_CLAUSE_BODY).
        assert_eq!(stmt.sql().matches("AND EXISTS (").count(), 1);
        assert!(stmt
            .sql()
            .contains("(lower(io2.io_type) = 'input' AND io2.token IN ("));
        assert!(stmt
            .sql()
            .contains("(lower(io2.io_type) = 'output' AND io2.token IN ("));
        assert!(stmt.sql().contains("\n          OR\n"));
        assert!(!stmt.sql().contains(OUTPUT_TOKENS_CLAUSE));
        assert!(!stmt.sql().contains(INPUT_TOKENS_CLAUSE));
        // Each canonical token is bound once per side of the OR.
        for token in [
            "0x00000000000000000000000000000000000000aa",
            "0x00000000000000000000000000000000000000bb",
        ] {
            assert_eq!(
                text_params(&stmt)
                    .into_iter()
                    .filter(|bound| bound == token)
                    .count(),
                2
            );
        }
    }

    // Differing input/output token sets produce TWO separate directional EXISTS
    // clauses (no OR-combining).
    #[test]
    fn differing_input_output_tokens_use_two_separate_exists() {
        let i = address!("0x00000000000000000000000000000000000000aa");
        let o = address!("0x00000000000000000000000000000000000000bb");
        let args = FetchOrdersArgs {
            tokens: FetchOrdersTokensFilter {
                inputs: vec![i],
                outputs: vec![o],
            },
            ..FetchOrdersArgs::default()
        };
        let (stmt, _) = bind(&args);
        assert_eq!(stmt.sql().matches("AND EXISTS (").count(), 2);
        assert!(stmt.sql().contains("AND lower(io2.io_type) = 'input'"));
        assert!(stmt.sql().contains("AND lower(io2.io_type) = 'output'"));
    }

    // has_positive_output_vault_balance == Some(true) injects the EXISTS clause
    // with a ZERO vault-id guard param; Some(false) and None strip it entirely.
    #[test]
    fn positive_balance_some_true_injects_exists_with_zero_vault_id() {
        let args = FetchOrdersArgs {
            has_positive_output_vault_balance: Some(true),
            ..FetchOrdersArgs::default()
        };
        let (stmt, _) = bind(&args);
        assert!(!stmt.sql().contains(POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE));
        assert!(stmt.sql().contains("FLOAT_GT_ZERO(vb_balance.balance)"));
        assert!(stmt.sql().contains("io_balance.vault_id != ?"));
        // The vault-id guard binds U256::ZERO as a 32-byte hex text param.
        assert!(text_params(&stmt).contains(
            &"0x0000000000000000000000000000000000000000000000000000000000000000".to_string()
        ));
    }

    #[test]
    fn positive_balance_some_false_strips_clause() {
        let args = FetchOrdersArgs {
            has_positive_output_vault_balance: Some(false),
            ..FetchOrdersArgs::default()
        };
        let (stmt, _) = bind(&args);
        assert!(!stmt.sql().contains(POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE));
        assert!(!stmt.sql().contains("FLOAT_GT_ZERO(vb_balance.balance)"));
    }

    #[test]
    fn positive_balance_none_strips_clause() {
        let args = FetchOrdersArgs::default();
        let (stmt, _) = bind(&args);
        assert!(!stmt.sql().contains(POSITIVE_OUTPUT_VAULT_BALANCE_CLAUSE));
        assert!(!stmt.sql().contains("FLOAT_GT_ZERO(vb_balance.balance)"));
    }

    // Some(hash) injects the COALESCE clause with a bound param; None strips the
    // marker.
    #[test]
    fn order_hash_some_binds_clause_none_strips() {
        use alloy::primitives::b256;
        let with = FetchOrdersArgs {
            order_hash: Some(b256!(
                "0x00000000000000000000000000000000000000000000000000000000deadbeef"
            )),
            ..FetchOrdersArgs::default()
        };
        let (stmt, _) = bind(&with);
        assert!(stmt
            .sql()
            .contains("AND COALESCE(la.order_hash, l.order_hash) = ?"));
        assert!(!stmt.sql().contains(ORDER_HASH_CLAUSE));
        assert!(text_params(&stmt).contains(
            &"0x00000000000000000000000000000000000000000000000000000000deadbeef".to_string()
        ));

        let without = FetchOrdersArgs::default();
        let (stmt2, _) = bind(&without);
        assert!(!stmt2.sql().contains(ORDER_HASH_CLAUSE));
        assert!(!stmt2
            .sql()
            .contains("AND COALESCE(la.order_hash, l.order_hash)"));
    }
}
