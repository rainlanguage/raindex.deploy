use crate::local_db::query::fetch_order_trades::LocalDbOrderTrade;
use crate::local_db::query::{SqlBuildError, SqlStatement, SqlValue};
use alloy::primitives::Address;

const QUERY_TEMPLATE: &str = include_str!("query.sql");
const RAINDEXES_CLAUSE: &str = "/*RAINDEXES_CLAUSE*/";
const RAINDEXES_CLAUSE_BODY: &str = "AND tws.raindex_address IN ({list})";
const PAIR_CLAUSE: &str = "/*PAIR_CLAUSE*/";

#[derive(Debug, Clone)]
pub struct FetchLatestTradesPerTokenArgs {
    pub chain_id: u32,
    pub raindex_addresses: Vec<Address>,
    pub quote_token: Address,
    pub base_tokens: Vec<Address>,
}

pub fn build_fetch_latest_trades_per_token_stmt(
    args: &FetchLatestTradesPerTokenArgs,
) -> Result<SqlStatement, SqlBuildError> {
    let mut stmt = SqlStatement::new(QUERY_TEMPLATE);

    let quote_partition = push_param(&mut stmt, args.quote_token);
    stmt.replace("/*QUOTE_PARTITION*/", &quote_partition)?;

    let chain_id = push_param(&mut stmt, args.chain_id);
    stmt.replace("/*CHAIN_ID*/", &chain_id)?;

    let mut raindexes = args.raindex_addresses.clone();
    raindexes.sort_unstable();
    raindexes.dedup();
    stmt.bind_list_clause(
        RAINDEXES_CLAUSE,
        RAINDEXES_CLAUSE_BODY,
        raindexes.into_iter().map(SqlValue::from),
    )?;

    let mut base_tokens = args.base_tokens.clone();
    base_tokens.sort_unstable();
    base_tokens.dedup();
    if base_tokens.is_empty() {
        stmt.replace(PAIR_CLAUSE, "AND 1 = 0")?;
        return Ok(stmt);
    }

    let input_quote = push_param(&mut stmt, args.quote_token);
    let output_bases = push_list(&mut stmt, &base_tokens);
    let output_quote = push_param(&mut stmt, args.quote_token);
    let input_bases = push_list(&mut stmt, &base_tokens);
    stmt.replace(
        PAIR_CLAUSE,
        &format!(
            "AND ((tws.input_token = {input_quote} AND tws.output_token IN ({output_bases})) \
             OR (tws.output_token = {output_quote} AND tws.input_token IN ({input_bases})))"
        ),
    )?;
    Ok(stmt)
}

fn push_param(stmt: &mut SqlStatement, value: impl Into<SqlValue>) -> String {
    let placeholder = format!("?{}", stmt.params().len() + 1);
    stmt.push(value.into());
    placeholder
}

fn push_list(stmt: &mut SqlStatement, values: &[Address]) -> String {
    values
        .iter()
        .map(|value| push_param(stmt, *value))
        .collect::<Vec<_>>()
        .join(", ")
}

pub type LatestTradeRow = LocalDbOrderTrade;

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn query_selects_one_latest_trade_per_base_token() {
        let quote = address!("2222222222222222222222222222222222222222");
        let base_a = address!("1111111111111111111111111111111111111111");
        let base_b = address!("3333333333333333333333333333333333333333");
        let raindex = address!("4444444444444444444444444444444444444444");

        let stmt = build_fetch_latest_trades_per_token_stmt(&FetchLatestTradesPerTokenArgs {
            chain_id: 8453,
            raindex_addresses: vec![raindex, raindex],
            quote_token: quote,
            base_tokens: vec![base_b, base_a, base_a],
        })
        .unwrap();

        assert!(stmt.sql.contains("ROW_NUMBER() OVER"));
        assert!(stmt.sql.contains("market_rank = 1"));
        assert!(stmt.sql.contains("tws.raindex_address IN (?3)"));
        assert!(stmt.sql.contains("tws.output_token IN (?5, ?6)"));
        assert!(stmt.sql.contains("tws.input_token IN (?8, ?9)"));
        assert_eq!(stmt.params().len(), 9);
        assert!(!stmt.sql.contains("/*"));
    }

    #[test]
    fn empty_base_tokens_build_a_match_none_query() {
        let stmt = build_fetch_latest_trades_per_token_stmt(&FetchLatestTradesPerTokenArgs {
            chain_id: 8453,
            raindex_addresses: vec![],
            quote_token: Address::ZERO,
            base_tokens: vec![],
        })
        .unwrap();

        assert!(stmt.sql.contains("AND 1 = 0"));
        assert!(!stmt.sql.contains("/*"));
    }
}
