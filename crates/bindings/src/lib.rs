use alloy::sol;

sol!(
    #![sol(all_derives = true, rpc)]
    #![sol(extra_derives(serde::Serialize, serde::Deserialize))]
    IRaindexV7, "./abis/IRaindexV7.json"
);

sol!(
    #![sol(all_derives = true)]
    #![sol(extra_derives(serde::Serialize, serde::Deserialize))]
    Raindex, "./abis/RaindexV7.json"
);

// Inline definition avoids non-deterministic artifact collision between
// forge-std's IERC20.sol (uses `amount`) and OpenZeppelin's IERC20.sol
// (uses `value`). Both produce out/IERC20.sol/IERC20.json and forge
// non-deterministically picks which one wins.
sol!(
    #![sol(all_derives = true, rpc)]
    interface IERC20 {
        event Transfer(address indexed from, address indexed to, uint256 value);
        event Approval(address indexed owner, address indexed spender, uint256 value);
        function totalSupply() external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
    }
);

sol!(
    #![sol(all_derives = true)]
    ERC20, "./abis/ERC20.json"
);

sol!(
    #![sol(all_derives = true, rpc)]
    IERC20Metadata, "./abis/IERC20Metadata.json"
);

sol!(
    #![sol(all_derives = true)]
    #![sol(extra_derives(serde::Serialize, serde::Deserialize))]
    IInterpreterStoreV3, "./abis/IInterpreterStoreV3.json"
);

pub mod provider;

#[cfg(target_family = "wasm")]
pub mod js_api;

#[cfg(target_family = "wasm")]
pub mod wasm_traits;

pub mod topics {
    use crate::{
        IInterpreterStoreV3::Set,
        IRaindexV7::{
            AddOrderV3, AfterClearV2, ClearV3, DepositV2, RemoveOrderV3, TakeOrderV4, WithdrawV2,
        },
        Raindex::MetaV1_2,
    };
    use alloy::{primitives::B256, sol_types::SolEvent};

    pub const RAINDEX_EVENT_TOPICS: [B256; 8] = [
        AddOrderV3::SIGNATURE_HASH,
        TakeOrderV4::SIGNATURE_HASH,
        WithdrawV2::SIGNATURE_HASH,
        DepositV2::SIGNATURE_HASH,
        RemoveOrderV3::SIGNATURE_HASH,
        ClearV3::SIGNATURE_HASH,
        AfterClearV2::SIGNATURE_HASH,
        MetaV1_2::SIGNATURE_HASH,
    ];

    pub const STORE_SET_TOPICS: [B256; 1] = [Set::SIGNATURE_HASH];
}
