use std::any::type_name;
use std::convert::TryFrom;

use cosmwasm_std::{
    Api, CanonicalAddr, Coin, HumanAddr, ReadonlyStorage, StdError, StdResult, Storage, Uint128,
};
use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage};

use secret_toolkit::storage::{AppendStore, AppendStoreMut, TypedStore, TypedStoreMut};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::msg::{status_level_to_u8, u8_to_status_level, ContractStatusLevel};
use crate::viewing_key::ViewingKey;
use serde::de::DeserializeOwned;

pub static CONFIG_KEY: &[u8] = b"config";
pub const PREFIX_TXS: &[u8] = b"transfers";

pub const KEY_CONSTANTS: &[u8] = b"constants";
pub const KEY_TOTAL_SUPPLY: &[u8] = b"total_supply";
pub const KEY_CONTRACT_STATUS: &[u8] = b"contract_status";
pub const KEY_MINTERS: &[u8] = b"minters";
pub const KEY_TX_COUNT: &[u8] = b"tx-count";
pub const KEY_MASTER_CONTRACT: &[u8] = b"mastercontract";

pub const PREFIX_CONFIG: &[u8] = b"config";
pub const PREFIX_BALANCES: &[u8] = b"balances";
pub const PREFIX_ALLOWANCES: &[u8] = b"allowances";
pub const PREFIX_VIEW_KEY: &[u8] = b"viewingkey";
pub const PREFIX_RECEIVERS: &[u8] = b"receivers";

// TypedStorage
pub const REWARD_BALANCE_KEY: &[u8] = b"rewardbalance";
pub const SEFI_KEY: &[u8] = b"sefi";

// Note that id is a globally incrementing counter.
// Since it's 64 bits long, even at 50 tx/s it would take
// over 11 billion years for it to rollback. I'm pretty sure
// we'll have bigger issues by then.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Tx {
    pub id: u64,
    pub from: HumanAddr,
    pub sender: HumanAddr,
    pub receiver: HumanAddr,
    pub coins: Coin,
}

impl Tx {
    pub fn into_stored<A: Api>(self, api: &A) -> StdResult<StoredTx> {
        let tx = StoredTx {
            id: self.id,
            from: api.canonical_address(&self.from)?,
            sender: api.canonical_address(&self.sender)?,
            receiver: api.canonical_address(&self.receiver)?,
            coins: self.coins,
        };
        Ok(tx)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredTx {
    pub id: u64,
    pub from: CanonicalAddr,
    pub sender: CanonicalAddr,
    pub receiver: CanonicalAddr,
    pub coins: Coin,
}

impl StoredTx {
    pub fn into_humanized<A: Api>(self, api: &A) -> StdResult<Tx> {
        let tx = Tx {
            id: self.id,
            from: api.human_address(&self.from)?,
            sender: api.human_address(&self.sender)?,
            receiver: api.human_address(&self.receiver)?,
            coins: self.coins,
        };
        Ok(tx)
    }
}

pub fn store_transfer<S: Storage>(
    store: &mut S,
    owner: &CanonicalAddr,
    sender: &CanonicalAddr,
    receiver: &CanonicalAddr,
    amount: Uint128,
    denom: String,
) -> StdResult<()> {
    let mut config = Config::from_storage(store);
    let id = config.tx_count() + 1;
    config.set_tx_count(id)?;

    let coins = Coin { denom, amount };
    let tx = StoredTx {
        id,
        from: owner.clone(),
        sender: sender.clone(),
        receiver: receiver.clone(),
        coins,
    };

    if owner != sender {
        append_tx(store, tx.clone(), &owner)?;
    }
    append_tx(store, tx.clone(), &sender)?;
    append_tx(store, tx, &receiver)?;

    Ok(())
}

fn append_tx<S: Storage>(
    store: &mut S,
    tx: StoredTx,
    for_address: &CanonicalAddr,
) -> StdResult<()> {
    let mut store = PrefixedStorage::multilevel(&[PREFIX_TXS, for_address.as_slice()], store);
    let mut store = AppendStoreMut::attach_or_create(&mut store)?;
    store.push(&tx)
}

pub fn get_transfers<A: Api, S: ReadonlyStorage>(
    api: &A,
    storage: &S,
    for_address: &CanonicalAddr,
    page: u32,
    page_size: u32,
) -> StdResult<Vec<Tx>> {
    let store = ReadonlyPrefixedStorage::multilevel(&[PREFIX_TXS, for_address.as_slice()], storage);

    // Try to access the storage of txs for the account.
    // If it doesn't exist yet, return an empty list of transfers.
    let store = if let Some(result) = AppendStore::<StoredTx, _>::attach(&store) {
        result?
    } else {
        return Ok(vec![]);
    };

    // Take `page_size` txs starting from the latest tx, potentially skipping `page * page_size`
    // txs from the start.
    let tx_iter = store
        .iter()
        .rev()
        .skip((page * page_size) as _)
        .take(page_size as _);
    // The `and_then` here flattens the `StdResult<StdResult<Tx>>` to an `StdResult<Tx>`
    let txs: StdResult<Vec<Tx>> = tx_iter
        .map(|tx| tx.map(|tx| tx.into_humanized(api)).and_then(|x| x))
        .collect();
    txs
}

// Config

#[derive(Serialize, Debug, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct Constants {
    pub name: String,
    pub admin: HumanAddr,
    pub symbol: String,
    pub decimals: u8,
    pub prng_seed: Vec<u8>,
    // privacy configuration
    pub total_supply_is_public: bool,
}

pub struct ReadonlyConfig<'a, S: ReadonlyStorage> {
    storage: ReadonlyPrefixedStorage<'a, S>,
}

impl<'a, S: ReadonlyStorage> ReadonlyConfig<'a, S> {
    pub fn from_storage(storage: &'a S) -> Self {
        Self {
            storage: ReadonlyPrefixedStorage::new(PREFIX_CONFIG, storage),
        }
    }

    fn as_readonly(&self) -> ReadonlyConfigImpl<ReadonlyPrefixedStorage<S>> {
        ReadonlyConfigImpl(&self.storage)
    }

    pub fn constants(&self) -> StdResult<Constants> {
        self.as_readonly().constants()
    }

    pub fn total_supply(&self) -> u128 {
        self.as_readonly().total_supply()
    }

    pub fn contract_status(&self) -> ContractStatusLevel {
        self.as_readonly().contract_status()
    }

    pub fn minters(&self) -> Vec<HumanAddr> {
        self.as_readonly().minters()
    }

    pub fn tx_count(&self) -> u64 {
        self.as_readonly().tx_count()
    }

    pub fn master_contract(&self) -> SecretContract {
        self.as_readonly().master_contract()
    }
}

fn set_bin_data<T: Serialize, S: Storage>(storage: &mut S, key: &[u8], data: &T) -> StdResult<()> {
    let bin_data =
        bincode2::serialize(&data).map_err(|e| StdError::serialize_err(type_name::<T>(), e))?;

    storage.set(key, &bin_data);
    Ok(())
}

fn get_bin_data<T: DeserializeOwned, S: ReadonlyStorage>(storage: &S, key: &[u8]) -> StdResult<T> {
    let bin_data = storage.get(key);

    match bin_data {
        None => Err(StdError::not_found("Key not found in storage")),
        Some(bin_data) => Ok(bincode2::deserialize::<T>(&bin_data)
            .map_err(|e| StdError::serialize_err(type_name::<T>(), e))?),
    }
}

pub struct Config<'a, S: Storage> {
    storage: PrefixedStorage<'a, S>,
}

impl<'a, S: Storage> Config<'a, S> {
    pub fn from_storage(storage: &'a mut S) -> Self {
        Self {
            storage: PrefixedStorage::new(PREFIX_CONFIG, storage),
        }
    }

    fn as_readonly(&self) -> ReadonlyConfigImpl<PrefixedStorage<S>> {
        ReadonlyConfigImpl(&self.storage)
    }

    pub fn constants(&self) -> StdResult<Constants> {
        self.as_readonly().constants()
    }

    pub fn set_constants(&mut self, constants: &Constants) -> StdResult<()> {
        set_bin_data(&mut self.storage, KEY_CONSTANTS, constants)
    }

    pub fn total_supply(&self) -> u128 {
        self.as_readonly().total_supply()
    }

    pub fn set_total_supply(&mut self, supply: u128) {
        self.storage.set(KEY_TOTAL_SUPPLY, &supply.to_be_bytes());
    }

    pub fn contract_status(&self) -> ContractStatusLevel {
        self.as_readonly().contract_status()
    }

    pub fn set_contract_status(&mut self, status: ContractStatusLevel) {
        let status_u8 = status_level_to_u8(status);
        self.storage
            .set(KEY_CONTRACT_STATUS, &status_u8.to_be_bytes());
    }

    pub fn set_minters(&mut self, minters_to_set: Vec<HumanAddr>) -> StdResult<()> {
        set_bin_data(&mut self.storage, KEY_MINTERS, &minters_to_set)
    }

    pub fn add_minters(&mut self, minters_to_add: Vec<HumanAddr>) -> StdResult<()> {
        let mut minters = self.minters();
        minters.extend(minters_to_add);

        self.set_minters(minters)
    }

    pub fn remove_minters(&mut self, minters_to_remove: Vec<HumanAddr>) -> StdResult<()> {
        let mut minters = self.minters();

        for minter in minters_to_remove {
            minters.retain(|x| x != &minter);
        }

        self.set_minters(minters)
    }

    pub fn minters(&mut self) -> Vec<HumanAddr> {
        self.as_readonly().minters()
    }

    pub fn tx_count(&self) -> u64 {
        self.as_readonly().tx_count()
    }

    pub fn set_tx_count(&mut self, count: u64) -> StdResult<()> {
        set_bin_data(&mut self.storage, KEY_TX_COUNT, &count)
    }

    pub fn set_master_contract(&mut self, master: SecretContract) -> StdResult<()> {
        set_bin_data(&mut self.storage, KEY_MASTER_CONTRACT, &master)
    }

    pub fn master_contract(&self) -> SecretContract {
        self.as_readonly().master_contract()
    }
}

/// This struct refactors out the readonly methods that we need for `Config` and `ReadonlyConfig`
/// in a way that is generic over their mutability.
///
/// This was the only way to prevent code duplication of these methods because of the way
/// that `ReadonlyPrefixedStorage` and `PrefixedStorage` are implemented in `cosmwasm-std`
struct ReadonlyConfigImpl<'a, S: ReadonlyStorage>(&'a S);

impl<'a, S: ReadonlyStorage> ReadonlyConfigImpl<'a, S> {
    fn constants(&self) -> StdResult<Constants> {
        let consts_bytes = self
            .0
            .get(KEY_CONSTANTS)
            .ok_or_else(|| StdError::generic_err("no constants stored in configuration"))?;
        bincode2::deserialize::<Constants>(&consts_bytes)
            .map_err(|e| StdError::serialize_err(type_name::<Constants>(), e))
    }

    fn total_supply(&self) -> u128 {
        let supply_bytes = self
            .0
            .get(KEY_TOTAL_SUPPLY)
            .expect("no total supply stored in config");
        // This unwrap is ok because we know we stored things correctly
        slice_to_u128(&supply_bytes).unwrap()
    }

    fn contract_status(&self) -> ContractStatusLevel {
        let supply_bytes = self
            .0
            .get(KEY_CONTRACT_STATUS)
            .expect("no contract status stored in config");

        // These unwraps are ok because we know we stored things correctly
        let status = slice_to_u8(&supply_bytes).unwrap();
        u8_to_status_level(status).unwrap()
    }

    fn minters(&self) -> Vec<HumanAddr> {
        get_bin_data(self.0, KEY_MINTERS).unwrap()
    }

    pub fn tx_count(&self) -> u64 {
        get_bin_data(self.0, KEY_TX_COUNT).unwrap_or_default()
    }

    fn master_contract(&self) -> SecretContract {
        get_bin_data(self.0, KEY_MASTER_CONTRACT).unwrap_or_default()
    }
}

// Balances

pub struct ReadonlyBalances<'a, S: ReadonlyStorage> {
    storage: ReadonlyPrefixedStorage<'a, S>,
}

impl<'a, S: ReadonlyStorage> ReadonlyBalances<'a, S> {
    pub fn from_storage(storage: &'a S) -> Self {
        Self {
            storage: ReadonlyPrefixedStorage::new(PREFIX_BALANCES, storage),
        }
    }

    fn as_readonly(&self) -> ReadonlyBalancesImpl<ReadonlyPrefixedStorage<S>> {
        ReadonlyBalancesImpl(&self.storage)
    }

    pub fn account_amount(&self, account: &CanonicalAddr) -> u128 {
        self.as_readonly().account_amount(account)
    }
}

pub struct Balances<'a, S: Storage> {
    storage: PrefixedStorage<'a, S>,
}

impl<'a, S: Storage> Balances<'a, S> {
    pub fn from_storage(storage: &'a mut S) -> Self {
        Self {
            storage: PrefixedStorage::new(PREFIX_BALANCES, storage),
        }
    }

    fn as_readonly(&self) -> ReadonlyBalancesImpl<PrefixedStorage<S>> {
        ReadonlyBalancesImpl(&self.storage)
    }

    pub fn balance(&self, account: &CanonicalAddr) -> u128 {
        self.as_readonly().account_amount(account)
    }

    pub fn set_account_balance(&mut self, account: &CanonicalAddr, amount: u128) {
        self.storage.set(account.as_slice(), &amount.to_be_bytes())
    }
}

/// This struct refactors out the readonly methods that we need for `Balances` and `ReadonlyBalances`
/// in a way that is generic over their mutability.
///
/// This was the only way to prevent code duplication of these methods because of the way
/// that `ReadonlyPrefixedStorage` and `PrefixedStorage` are implemented in `cosmwasm-std`
struct ReadonlyBalancesImpl<'a, S: ReadonlyStorage>(&'a S);

impl<'a, S: ReadonlyStorage> ReadonlyBalancesImpl<'a, S> {
    pub fn account_amount(&self, account: &CanonicalAddr) -> u128 {
        let account_bytes = account.as_slice();
        let result = self.0.get(account_bytes);
        match result {
            // This unwrap is ok because we know we stored things correctly
            Some(balance_bytes) => slice_to_u128(&balance_bytes).unwrap(),
            None => 0,
        }
    }
}

// Allowances

#[derive(Serialize, Debug, Deserialize, Clone, PartialEq, Default, JsonSchema)]
pub struct Allowance {
    pub amount: u128,
    pub expiration: Option<u64>,
}

pub fn read_allowance<S: Storage>(
    store: &S,
    owner: &CanonicalAddr,
    spender: &CanonicalAddr,
) -> StdResult<Allowance> {
    let owner_store =
        ReadonlyPrefixedStorage::multilevel(&[PREFIX_ALLOWANCES, owner.as_slice()], store);
    let owner_store = TypedStore::attach(&owner_store);
    let allowance = owner_store.may_load(spender.as_slice());
    allowance.map(Option::unwrap_or_default)
}

pub fn write_allowance<S: Storage>(
    store: &mut S,
    owner: &CanonicalAddr,
    spender: &CanonicalAddr,
    allowance: Allowance,
) -> StdResult<()> {
    let mut owner_store =
        PrefixedStorage::multilevel(&[PREFIX_ALLOWANCES, owner.as_slice()], store);
    let mut owner_store = TypedStoreMut::attach(&mut owner_store);

    owner_store.store(spender.as_slice(), &allowance)
}

// Viewing Keys

pub fn write_viewing_key<S: Storage>(store: &mut S, owner: &CanonicalAddr, key: &ViewingKey) {
    let mut balance_store = PrefixedStorage::new(PREFIX_VIEW_KEY, store);
    balance_store.set(owner.as_slice(), &key.to_hashed());
}

pub fn read_viewing_key<S: Storage>(store: &S, owner: &CanonicalAddr) -> Option<Vec<u8>> {
    let balance_store = ReadonlyPrefixedStorage::new(PREFIX_VIEW_KEY, store);
    balance_store.get(owner.as_slice())
}

// Receiver Interface

pub fn get_receiver_hash<S: ReadonlyStorage>(
    store: &S,
    account: &HumanAddr,
) -> Option<StdResult<String>> {
    let store = ReadonlyPrefixedStorage::new(PREFIX_RECEIVERS, store);
    store.get(account.as_str().as_bytes()).map(|data| {
        String::from_utf8(data)
            .map_err(|_err| StdError::invalid_utf8("stored code hash was not a valid String"))
    })
}

pub fn set_receiver_hash<S: Storage>(store: &mut S, account: &HumanAddr, code_hash: String) {
    let mut store = PrefixedStorage::new(PREFIX_RECEIVERS, store);
    store.set(account.as_str().as_bytes(), code_hash.as_bytes());
}

// Helpers

/// Converts 16 bytes value into u128
/// Errors if data found that is not 16 bytes
fn slice_to_u128(data: &[u8]) -> StdResult<u128> {
    match <[u8; 16]>::try_from(data) {
        Ok(bytes) => Ok(u128::from_be_bytes(bytes)),
        Err(_) => Err(StdError::generic_err(
            "Corrupted data found. 16 byte expected.",
        )),
    }
}

/// Converts 1 byte value into u8
/// Errors if data found that is not 1 byte
fn slice_to_u8(data: &[u8]) -> StdResult<u8> {
    if data.len() == 1 {
        Ok(data[0])
    } else {
        Err(StdError::generic_err(
            "Corrupted data found. 1 byte expected.",
        ))
    }
}
