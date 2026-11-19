use alloy::{
    contract::SolCallBuilder,
    hex::decode,
    network::{AnyNetwork, AnyTransactionReceipt, EthereumWallet, TransactionBuilder},
    node_bindings::{Anvil, AnvilInstance},
    primitives::{address, utils::parse_units, Address, Bytes, B256, U256},
    providers::{ext::AnvilApi, PendingTransactionError, Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    serde::WithOtherFields,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolCall,
    transports::{RpcError, TransportErrorKind},
};
pub use rain_interpreter_test_fixtures::{
    Deployer, Interpreter, Parser, RainlangContract, Store, ERC20,
};
use serde_json::value::RawValue;
use std::marker::PhantomData;

use rain_interpreter_test_fixtures::LocalEvmProvider;
use rain_math_float::Float;

sol!(
    #![sol(all_derives = true, rpc = true)]
    Raindex, "./abis/RaindexV7.json"
);

sol!(
    #![sol(all_derives = true, rpc = true)]
    RaindexSubParser, "./abis/RaindexV7SubParser.json"
);

sol!(
    #![sol(all_derives = true, rpc = true)]
    "./contracts/IMulticall3.sol"
);

/// A local evm instance that wraps an Anvil instance and provider with
/// its signers, and with rain contracts already deployed on it.
/// The first signer wallet is the main wallet that would sign any transactions
/// that dont specify a sender ('to' field)
pub struct LocalEvm {
    /// The Anvil instance, ie the local blockchain
    pub anvil: AnvilInstance,

    /// The alloy provider instance of this local blockchain
    pub provider: LocalEvmProvider,

    /// Alloy raindex contract instance deployed on this blockchain
    pub raindex: Raindex::RaindexInstance<LocalEvmProvider, AnyNetwork>,

    /// Alloy raindex subparser contract instance deployed on this blockchain
    pub raindex_subparser: RaindexSubParser::RaindexSubParserInstance<LocalEvmProvider, AnyNetwork>,

    /// Alloy interpreter contract instance deployed on this blockchain
    pub interpreter: Interpreter::InterpreterInstance<LocalEvmProvider, AnyNetwork>,

    /// Alloy store contract instance deployed on this blockchain
    pub store: Store::StoreInstance<LocalEvmProvider, AnyNetwork>,

    /// Alloy parser contract instance deployed on this blockchain
    pub parser: Parser::ParserInstance<LocalEvmProvider, AnyNetwork>,

    /// Alloy expression deployer contract instance deployed on this blockchain
    pub deployer: Deployer::DeployerInstance<LocalEvmProvider, AnyNetwork>,

    /// Alloy multicall3 contract instance deployed at the official multicall3 address on this blockchain
    pub multicall3: IMulticall3::IMulticall3Instance<LocalEvmProvider, AnyNetwork>,

    /// Array of alloy ERC20 contract instances deployed on this blockchain
    pub tokens: Vec<ERC20::ERC20Instance<LocalEvmProvider, AnyNetwork>>,

    /// Address of the deployed Rainlang instance
    pub rainlang: Address,

    /// All wallets of this local blockchain that can be used to perform transactions
    /// the first wallet is the blockchain's default wallet, ie transactions that dont
    /// explicitly specify a sender address will use this as the sender
    pub signer_wallets: Vec<EthereumWallet>,
}

impl LocalEvm {
    /// Instantiates this struct with rain contracts deployed and no ERC20 tokens
    pub async fn new() -> Self {
        let anvil = Anvil::new()
            .arg("--disable-code-size-limit")
            .try_spawn()
            .unwrap();

        // set up signers from anvil accounts
        let mut signer_wallets = vec![];
        let mut default_signer =
            EthereumWallet::from(PrivateKeySigner::from(anvil.keys()[0].clone()));
        let other_signer_wallets: Vec<EthereumWallet> = anvil.keys()[1..]
            .iter()
            .map(|v| EthereumWallet::from(PrivateKeySigner::from(v.clone())))
            .collect();

        for s in &other_signer_wallets {
            default_signer.register_signer(s.default_signer())
        }
        signer_wallets.push(default_signer);
        signer_wallets.extend(other_signer_wallets);

        // Create a provider with the wallet and fillers
        let provider = ProviderBuilder::new_with_network::<AnyNetwork>()
            .wallet(signer_wallets[0].clone())
            .connect_http(anvil.endpoint_url());

        // Deploy the canonical TOFUTokenDecimals creation code (from
        // rain-tofu-erc20-decimals' LibTOFUTokenDecimals) to obtain its runtime,
        // then place that at the deterministic singleton address. The lib's
        // `ensureDeployed` checks the codehash against TOFU_DECIMALS_EXPECTED_CODE_HASH,
        // which raindex's own compile of TOFUTokenDecimals.sol does not match.
        let tofu_deployed = provider
            .send_transaction(WithOtherFields::new(
                TransactionRequest::default()
                    .with_deploy_code(decode(TOFU_DECIMALS_CREATION_CODE).unwrap()),
            ))
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap()
            .contract_address
            .unwrap();
        let tofu_runtime = provider.get_code_at(tofu_deployed).await.unwrap();
        provider
            .anvil_set_code(TOFU_TOKEN_DECIMALS_ADDRESS, tofu_runtime)
            .await
            .unwrap();

        // Deploy the registry and query deterministic addresses
        let registry_instance = RainlangContract::deploy(provider.clone()).await.unwrap();
        let registry_addr = *registry_instance.address();
        let parser_addr = registry_instance.parserAddress().call().await.unwrap();
        let store_addr = registry_instance.storeAddress().call().await.unwrap();
        let interpreter_addr = registry_instance.interpreterAddress().call().await.unwrap();
        let deployer_addr = registry_instance
            .expressionDeployerAddress()
            .call()
            .await
            .unwrap();

        // Set component bytecodes at their deterministic addresses
        provider
            .anvil_set_code(parser_addr, Parser::DEPLOYED_BYTECODE.clone())
            .await
            .unwrap();
        provider
            .anvil_set_code(store_addr, Store::DEPLOYED_BYTECODE.clone())
            .await
            .unwrap();
        provider
            .anvil_set_code(interpreter_addr, Interpreter::DEPLOYED_BYTECODE.clone())
            .await
            .unwrap();
        provider
            .anvil_set_code(deployer_addr, Deployer::DEPLOYED_BYTECODE.clone())
            .await
            .unwrap();

        // deploy raindex contracts
        let raindex = Raindex::deploy(provider.clone()).await.unwrap();
        let raindex_subparser = RaindexSubParser::deploy(provider.clone()).await.unwrap();

        // set the multicall 3 contract at its official address
        provider
            .anvil_set_code(
                MULTICALL3_ADDRESS,
                decode(MULTICALL3_BYTECODE).unwrap().into(),
            )
            .await
            .unwrap();

        let interpreter = Interpreter::new(interpreter_addr, provider.clone());
        let store = Store::new(store_addr, provider.clone());
        let parser = Parser::new(parser_addr, provider.clone());
        let deployer = Deployer::new(deployer_addr, provider.clone());
        let multicall3 = IMulticall3::new(MULTICALL3_ADDRESS, provider.clone());

        Self {
            anvil,
            provider,
            raindex,
            raindex_subparser,
            interpreter,
            store,
            parser,
            deployer,
            multicall3,
            tokens: vec![],
            rainlang: registry_addr,
            signer_wallets,
        }
    }

    /// Instantiates with number of ERC20 tokens with 18 decimals.
    /// Each token after being deployed will mint 1 milion tokens to the
    /// default address, which is the first signer wallet of this instance
    pub async fn new_with_tokens(token_count: u8) -> Self {
        let mut local_evm = Self::new().await;

        // deploy tokens contracts and mint 1 milion of each for the default address (first signer wallet)
        for i in 1..=token_count {
            local_evm
                .deploy_new_token(
                    &format!("Token{i}"),
                    &format!("TOKEN{i}"),
                    18,
                    parse_units("1_000_000", 18).unwrap().into(),
                    local_evm.anvil.addresses()[0],
                )
                .await;
        }
        local_evm
    }

    /// Get the local rpc url of the underlying anvil instance
    pub fn url(&self) -> String {
        self.anvil.endpoint()
    }

    /// Deploys a new ERC20 token with the given arguments
    pub async fn deploy_new_token(
        &mut self,
        name: &str,
        symbol: &str,
        decimals: u8,
        supply: U256,
        recipient: Address,
    ) -> ERC20::ERC20Instance<LocalEvmProvider, AnyNetwork> {
        let token = ERC20::deploy(
            self.provider.clone(),
            name.to_string(),
            symbol.to_string(),
            decimals,
            recipient,
            supply,
        )
        .await
        .unwrap();
        self.tokens.push(token.clone());
        token
    }

    /// Sends a contract write transaction to the blockchain and returns the tx receipt
    pub async fn send_contract_transaction<T: SolCall>(
        &self,
        contract_call: SolCallBuilder<&LocalEvmProvider, PhantomData<T>, AnyNetwork>,
    ) -> Result<AnyTransactionReceipt, PendingTransactionError> {
        self.provider
            .send_transaction(contract_call.into_transaction_request())
            .await?
            .get_receipt()
            .await
    }

    /// Sends (write call) a transaction request to the blockchain and returns the tx receipt
    pub async fn send_transaction(
        &self,
        tx: WithOtherFields<TransactionRequest>,
    ) -> Result<AnyTransactionReceipt, PendingTransactionError> {
        self.provider
            .send_transaction(tx)
            .await?
            .get_receipt()
            .await
    }

    /// Calls (readonly call) contract method and returns the decoded result
    pub async fn call_contract<T: SolCall>(
        &self,
        contract_call: SolCallBuilder<&LocalEvmProvider, T, AnyNetwork>,
    ) -> Result<Result<T::Return, alloy::sol_types::Error>, RpcError<TransportErrorKind>> {
        Ok(T::abi_decode_returns(
            &self
                .provider
                .call(contract_call.clone().into_transaction_request())
                .await?,
        ))
    }

    /// Calls (readonly call) a raw transaction and returns the result
    pub async fn call(
        &self,
        tx: WithOtherFields<TransactionRequest>,
    ) -> Result<Bytes, RpcError<TransportErrorKind, Box<RawValue>>> {
        self.provider.call(tx).await
    }

    /// Adds an order with given calldata and deposit the specified amount into the given token vault,
    /// returns the AddOrder event and addOrder2() and deposit2() transaction receipts
    pub async fn add_order_and_deposit(
        &self,
        add_order_calldata: &[u8],
        from: Address,
        token: Address,
        deposit_amount: U256,
        decimals: u8,
        vault_id: B256,
    ) -> (
        Raindex::AddOrderV3,
        AnyTransactionReceipt,
        AnyTransactionReceipt,
    ) {
        // add the order
        let (log, tx1) = self.add_order(add_order_calldata, from).await;
        // deposit
        let tx2 = self
            .deposit(from, token, deposit_amount, decimals, vault_id)
            .await;
        (log, tx1, tx2)
    }

    /// Adds an order with given calldata, returns the AddOrder event and addOrder2() transaction receipts
    pub async fn add_order(
        &self,
        add_order_calldata: &[u8],
        from: Address,
    ) -> (Raindex::AddOrderV3, AnyTransactionReceipt) {
        let tx_req = WithOtherFields::new(
            TransactionRequest::default()
                .with_input(add_order_calldata.to_vec())
                .with_to(*self.raindex.address())
                .with_from(from),
        );

        let tx = self.send_transaction(tx_req).await.unwrap();

        // decode the logs to get AddOrderV2 event struct
        let log = tx
            .inner
            .inner
            .logs()
            .iter()
            .find_map(|v| v.log_decode::<Raindex::AddOrderV3>().ok())
            .unwrap()
            .inner
            .data;

        (log, tx)
    }

    /// Deposit the specified amount into the given token vault, returns the deposit2() transaction receipts
    pub async fn deposit(
        &self,
        from: Address,
        token: Address,
        deposit_amount: U256,
        decimals: u8,
        vault_id: B256,
    ) -> AnyTransactionReceipt {
        // approve and deposit
        let token_contract = self
            .tokens
            .iter()
            .find(|v| *v.address() == token)
            .expect("Token with given address is not deployed");

        let tx = token_contract
            .approve(*self.raindex.address(), deposit_amount)
            .from(from)
            .into_transaction_request();

        self.send_transaction(tx).await.unwrap();

        let raw_deposit_amount = Float::from_fixed_decimal(deposit_amount, decimals)
            .unwrap()
            .get_inner();

        let tx = self
            .raindex
            .deposit5(token, vault_id, raw_deposit_amount, vec![])
            .from(from)
            .into_transaction_request();

        self.send_transaction(tx).await.unwrap()
    }

    /// Calls the contract without commiting a transction (readonly call) and returns the result
    pub async fn call_and_decode<T: SolCall>(
        &self,
        tx_req: WithOtherFields<TransactionRequest>,
    ) -> Result<Result<T::Return, alloy::sol_types::Error>, RpcError<TransportErrorKind>> {
        let returns = self.provider.call(tx_req).await?;
        Ok(T::abi_decode_returns(&returns))
    }
}

const MULTICALL3_ADDRESS: Address = address!("cA11bde05977b3631167028862bE2a173976CA11");
const MULTICALL3_BYTECODE: &str = "0x6080604052600436106100f35760003560e01c80634d2301cc1161008a578063a8b0574e11610059578063a8b0574e1461025a578063bce38bd714610275578063c3077fa914610288578063ee82ac5e1461029b57600080fd5b80634d2301cc146101ec57806372425d9d1461022157806382ad56cb1461023457806386d516e81461024757600080fd5b80633408e470116100c65780633408e47014610191578063399542e9146101a45780633e64a696146101c657806342cbb15c146101d957600080fd5b80630f28c97d146100f8578063174dea711461011a578063252dba421461013a57806327e86d6e1461015b575b600080fd5b34801561010457600080fd5b50425b6040519081526020015b60405180910390f35b61012d610128366004610a85565b6102ba565b6040516101119190610bbe565b61014d610148366004610a85565b6104ef565b604051610111929190610bd8565b34801561016757600080fd5b50437fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0140610107565b34801561019d57600080fd5b5046610107565b6101b76101b2366004610c60565b610690565b60405161011193929190610cba565b3480156101d257600080fd5b5048610107565b3480156101e557600080fd5b5043610107565b3480156101f857600080fd5b50610107610207366004610ce2565b73ffffffffffffffffffffffffffffffffffffffff163190565b34801561022d57600080fd5b5044610107565b61012d610242366004610a85565b6106ab565b34801561025357600080fd5b5045610107565b34801561026657600080fd5b50604051418152602001610111565b61012d610283366004610c60565b61085a565b6101b7610296366004610a85565b610a1a565b3480156102a757600080fd5b506101076102b6366004610d18565b4090565b60606000828067ffffffffffffffff8111156102d8576102d8610d31565b60405190808252806020026020018201604052801561031e57816020015b6040805180820190915260008152606060208201528152602001906001900390816102f65790505b5092503660005b8281101561047757600085828151811061034157610341610d60565b6020026020010151905087878381811061035d5761035d610d60565b905060200281019061036f9190610d8f565b6040810135958601959093506103886020850185610ce2565b73ffffffffffffffffffffffffffffffffffffffff16816103ac6060870187610dcd565b6040516103ba929190610e32565b60006040518083038185875af1925050503d80600081146103f7576040519150601f19603f3d011682016040523d82523d6000602084013e6103fc565b606091505b50602080850191909152901515808452908501351761046d577f08c379a000000000000000000000000000000000000000000000000000000000600052602060045260176024527f4d756c746963616c6c333a2063616c6c206661696c656400000000000000000060445260846000fd5b5050600101610325565b508234146104e6576040517f08c379a000000000000000000000000000000000000000000000000000000000815260206004820152601a60248201527f4d756c746963616c6c333a2076616c7565206d69736d6174636800000000000060448201526064015b60405180910390fd5b50505092915050565b436060828067ffffffffffffffff81111561050c5761050c610d31565b60405190808252806020026020018201604052801561053f57816020015b606081526020019060019003908161052a5790505b5091503660005b8281101561068657600087878381811061056257610562610d60565b90506020028101906105749190610e42565b92506105836020840184610ce2565b73ffffffffffffffffffffffffffffffffffffffff166105a66020850185610dcd565b6040516105b4929190610e32565b6000604051808303816000865af19150503d80600081146105f1576040519150601f19603f3d011682016040523d82523d6000602084013e6105f6565b606091505b5086848151811061060957610609610d60565b602090810291909101015290508061067d576040517f08c379a000000000000000000000000000000000000000000000000000000000815260206004820152601760248201527f4d756c746963616c6c333a2063616c6c206661696c656400000000000000000060448201526064016104dd565b50600101610546565b5050509250929050565b43804060606106a086868661085a565b905093509350939050565b6060818067ffffffffffffffff8111156106c7576106c7610d31565b60405190808252806020026020018201604052801561070d57816020015b6040805180820190915260008152606060208201528152602001906001900390816106e55790505b5091503660005b828110156104e657600084828151811061073057610730610d60565b6020026020010151905086868381811061074c5761074c610d60565b905060200281019061075e9190610e76565b925061076d6020840184610ce2565b73ffffffffffffffffffffffffffffffffffffffff166107906040850185610dcd565b60405161079e929190610e32565b6000604051808303816000865af19150503d80600081146107db576040519150601f19603f3d011682016040523d82523d6000602084013e6107e0565b606091505b506020808401919091529015158083529084013517610851577f08c379a000000000000000000000000000000000000000000000000000000000600052602060045260176024527f4d756c746963616c6c333a2063616c6c206661696c656400000000000000000060445260646000fd5b50600101610714565b6060818067ffffffffffffffff81111561087657610876610d31565b6040519080825280602002602001820160405280156108bc57816020015b6040805180820190915260008152606060208201528152602001906001900390816108945790505b5091503660005b82811015610a105760008482815181106108df576108df610d60565b602002602001015190508686838181106108fb576108fb610d60565b905060200281019061090d9190610e42565b925061091c6020840184610ce2565b73ffffffffffffffffffffffffffffffffffffffff1661093f6020850185610dcd565b60405161094d929190610e32565b6000604051808303816000865af19150503d806000811461098a576040519150601f19603f3d011682016040523d82523d6000602084013e61098f565b606091505b506020830152151581528715610a07578051610a07576040517f08c379a000000000000000000000000000000000000000000000000000000000815260206004820152601760248201527f4d756c746963616c6c333a2063616c6c206661696c656400000000000000000060448201526064016104dd565b506001016108c3565b5050509392505050565b6000806060610a2b60018686610690565b919790965090945092505050565b60008083601f840112610a4b57600080fd5b50813567ffffffffffffffff811115610a6357600080fd5b6020830191508360208260051b8501011115610a7e57600080fd5b9250929050565b60008060208385031215610a9857600080fd5b823567ffffffffffffffff811115610aaf57600080fd5b610abb85828601610a39565b90969095509350505050565b6000815180845260005b81811015610aed57602081850181015186830182015201610ad1565b81811115610aff576000602083870101525b50601f017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0169290920160200192915050565b600082825180855260208086019550808260051b84010181860160005b84811015610bb1578583037fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe001895281518051151584528401516040858501819052610b9d81860183610ac7565b9a86019a9450505090830190600101610b4f565b5090979650505050505050565b602081526000610bd16020830184610b32565b9392505050565b600060408201848352602060408185015281855180845260608601915060608160051b870101935082870160005b82811015610c52577fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffa0888703018452610c40868351610ac7565b95509284019290840190600101610c06565b509398975050505050505050565b600080600060408486031215610c7557600080fd5b83358015158114610c8557600080fd5b9250602084013567ffffffffffffffff811115610ca157600080fd5b610cad86828701610a39565b9497909650939450505050565b838152826020820152606060408201526000610cd96060830184610b32565b95945050505050565b600060208284031215610cf457600080fd5b813573ffffffffffffffffffffffffffffffffffffffff81168114610bd157600080fd5b600060208284031215610d2a57600080fd5b5035919050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052604160045260246000fd5b7f4e487b7100000000000000000000000000000000000000000000000000000000600052603260045260246000fd5b600082357fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff81833603018112610dc357600080fd5b9190910192915050565b60008083357fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe1843603018112610e0257600080fd5b83018035915067ffffffffffffffff821115610e1d57600080fd5b602001915036819003821315610a7e57600080fd5b8183823760009101908152919050565b600082357fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc1833603018112610dc357600080fd5b600082357fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffa1833603018112610dc357600080fdfea2646970667358221220bb2b5c71a328032f97c676ae39a1ec2148d3e5d6f73d95e9b17910152d61f16264736f6c634300080c0033";

const TOFU_TOKEN_DECIMALS_ADDRESS: Address = address!("0x200e12D10bb0c5E4a17e7018f0F1161919bb9389");

/// Canonical TOFUTokenDecimals creation code, copied verbatim from
/// rain-tofu-erc20-decimals' `LibTOFUTokenDecimals.TOFU_DECIMALS_EXPECTED_CREATION_CODE`.
/// Deploying this produces the runtime whose codehash equals the lib's
/// `TOFU_DECIMALS_EXPECTED_CODE_HASH`, which `ensureDeployed` requires.
const TOFU_DECIMALS_CREATION_CODE: &str = "6080604052348015600e575f80fd5b5061044b8061001c5f395ff3fe608060405234801561000f575f80fd5b506004361061004a575f3560e01c80630782d7e11461004e57806354636d2b14610078578063b7bad1b11461009d578063f5c36eaf146100b0575b5f80fd5b61006161005c366004610363565b6100c3565b60405161006f929190610403565b60405180910390f35b61008b610086366004610363565b6100d8565b60405160ff909116815260200161006f565b6100616100ab366004610363565b6100e9565b61008b6100be366004610363565b6100f5565b5f806100cf5f84610100565b91509150915091565b5f6100e35f836101f0565b92915050565b5f806100cf5f84610281565b5f6100e35f83610356565b73ffffffffffffffffffffffffffffffffffffffff81165f9081526020838152604080832081518083019092525460ff8082161515835261010090910416818301527f313ce56700000000000000000000000000000000000000000000000000000000808452839283908190816004818a5afa915060203d1015610182575f91505b811561019857505f5160ff811115610198575f91505b816101af57505050602001516003925090506101e9565b83516101c3575f955093506101e992505050565b836020015160ff1681146101d85760026101db565b60015b846020015195509550505050505b9250929050565b5f805f6101fd8585610281565b909250905060018260038111156102165761021661039d565b1415801561023557505f8260038111156102325761023261039d565b14155b156102795783826040517fee07877f000000000000000000000000000000000000000000000000000000008152600401610270929190610421565b60405180910390fd5b949350505050565b5f805f8061028f8686610100565b90925090505f8260038111156102a7576102a761039d565b0361034b576040805180820182526001815260ff838116602080840191825273ffffffffffffffffffffffffffffffffffffffff8a165f908152908b9052939093209151825493517fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00009094169015157fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00ff161761010093909116929092029190911790555b909590945092505050565b5f805f6101fd8585610100565b5f60208284031215610373575f80fd5b813573ffffffffffffffffffffffffffffffffffffffff81168114610396575f80fd5b9392505050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52602160045260245ffd5b600481106103ff577f4e487b71000000000000000000000000000000000000000000000000000000005f52602160045260245ffd5b9052565b6040810161041182856103ca565b60ff831660208301529392505050565b73ffffffffffffffffffffffffffffffffffffffff831681526040810161039660208301846103ca56";
