#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://substrate.dev/docs/en/knowledgebase/runtime/frame>
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use core::fmt;
use frame_support::{
//	decl_error, decl_event, decl_module, decl_storage, dispatch::DispatchResult,
	traits::{ Currency, ExistenceRequirement },
};
use parity_scale_codec::{Decode, Encode};

use frame_system::{
	self as system, ensure_signed,
	offchain::{
		AppCrypto, CreateSignedTransaction, SendSignedTransaction,
		SignedPayload, SigningTypes, Signer,
	},
};
use sp_core::crypto::KeyTypeId;
use sp_runtime::{
	AccountId32,
	RuntimeDebug,
	offchain::{
		http,
		storage::StorageValueRef,
	},
};
use sp_std::{
	prelude::*, str,
};
use alt_serde::{Deserialize, Deserializer};
use chrono::{DateTime, Duration};
use rustc_hex::FromHex;
use sp_runtime::SaturatedConversion;

// genesis settings.
// Definition of the interval at which data is retrieved from the Github Issue site used as a faucet front end.
const FAUCET_CHECK_INTERVAL: u64 = 60000;
// Amount of test net token to send at once.
pub const TOKEN_AMOUNT: u64 = 1000000000000000;
// KeyType definition. 
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"shin");
// Specify the block number as the interval until the account that received the test net token can receive.
pub const WAIT_BLOCK_NUMBER: u32 = 1000; 
// HTTP_USER_AGENT string.
const HTTP_HEADER_USER_AGENT: &str = "realtakahashi";
// URL of Github Issue to use as front end.
const HTTP_REMOTE_REQUEST: &str = "https://api.github.com/repos/realtakahashi/faucet_pallet/issues/2/comments";

pub mod crypto {
	use crate::KEY_TYPE;
	use sp_core::sr25519::Signature as Sr25519Signature;
	use sp_runtime::app_crypto::{app_crypto, sr25519};
	use sp_runtime::{
		traits::Verify,
		MultiSignature, MultiSigner,
	};

	app_crypto!(sr25519, KEY_TYPE);

	pub struct TestAuthId;

	impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}

	// implemented for mock runtime in test
	impl frame_system::offchain::AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature>
		for TestAuthId
	{
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Payload<Public> {
	number: u64,
	public: Public
}

impl <T: SigningTypes> SignedPayload<T> for Payload<T::Public> {
	fn public(&self) -> T::Public {
		self.public.clone()
	}
}

#[derive(Clone, Eq, PartialEq, Default, Encode, Decode, Hash, Debug)]
pub struct FaucetData {
    pub id: u64,
	pub login: Vec<u8>,
    pub created_at: Vec<u8>,
	pub address: Vec<u8>,
}

type Balance<T> = <<T as Config>::Currency as Currency<<T as system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>> {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
		type Call: From<Call<Self>>;
		type Currency: Currency<Self::AccountId>;
//		type AccountId = <Self as pallet::Config>::AccountId;	
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(_block_number: T::BlockNumber) {
			Self::check_faucet_datas(LatestFaucetData::<T>::get());
		}
	}

	// The pallet's runtime storage items.
	// https://substrate.dev/docs/en/knowledgebase/runtime/storage

	#[pallet::storage]
	#[pallet::getter(fn latest_faucet_data)]
	pub(super) type LatestFaucetData<T> = StorageValue<_, FaucetData, ValueQuery>;

	// pub type Sendlist<T>: map hasher(blake2_128_concat) T::AccountId => Option<<T as frame_system::Config>::BlockNumber>;
	#[pallet::storage]
	#[pallet::getter(fn send_list)]
	pub(super) type Sendlist<T> = StorageMap<Hasher = Blake2_128Concat, Key = <T as system::Config>::AccountId , Value = <T as frame_system::Config>::BlockNumber>;

	// Pallets use events to inform users when important changes are made.
	// https://substrate.dev/docs/en/knowledgebase/runtime/events
	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
		// SomethingStored(u32, T::AccountId),
		TestNetTokenTransfered(<T as system::Config>::AccountId),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		// NoneValue,
		/// Errors should have helpful documentation associated with them.
		// StorageOverflow,
		HttpFetchingError,
		OffchainSignedTxError, 
		NoLocalAcctForSigning, 
		TimeHasNotPassed,
		StrConvertError,
		TransferTokenError,
		HTTPFetchError,
	}


	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		// #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		// pub fn do_something(origin: OriginFor<T>, something: u32) -> DispatchResult {
		// 	// Check that the extrinsic was signed and get the signer.
		// 	// This function will return an error if the extrinsic is not signed.
		// 	// https://substrate.dev/docs/en/knowledgebase/runtime/origin
		// 	let who = ensure_signed(origin)?;

		// 	// Update storage.
		// 	<Something<T>>::put(something);

		// 	// Emit an event.
		// 	Self::deposit_event(Event::SomethingStored(something, who));
		// 	// Return a successful DispatchResultWithPostInfo
		// 	Ok(())
		// }

		// /// An example dispatchable that may throw a custom error.
		// #[pallet::weight(10_000 + T::DbWeight::get().reads_writes(1,1))]
		// pub fn cause_error(origin: OriginFor<T>) -> DispatchResult {
		// 	let _who = ensure_signed(origin)?;

		// 	// Read a value from storage.
		// 	match <Something<T>>::get() {
		// 		// Return an error if the value has not been set.
		// 		None => Err(Error::<T>::NoneValue)?,
		// 		Some(old) => {
		// 			// Increment the value read from storage; will error in the event of overflow.
		// 			let new = old.checked_add(1).ok_or(Error::<T>::StorageOverflow)?;
		// 			// Update the value in storage with the incremented result.
		// 			<Something<T>>::put(new);
		// 			Ok(())
		// 		},
		// 	}
		// }
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn send_some_testnet_token(origin: OriginFor<T>, faucet_datas: Vec<FaucetData>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let data = LatestFaucetData::<T>::get();
			let tmp = data.created_at.clone();
			let latest_created_at_str = str::from_utf8(&tmp).map_err(|_| Error::<T>::StrConvertError)?;
			let latest_id = data.id;

			// match LatestFaucetData::get(){
			// 	Some(data) => {
			// 		tmp = data.created_at.clone();
			// 		latest_created_at_str = str::from_utf8(&tmp).map_err(|_| Error::<T>::StrConvertError)?;
			// 		latest_id = data.id;
			// 	},
			// 	None => {
			// 		latest_created_at_str = "1976-09-24T16:00:00Z";
			// 		latest_id = 0;	
			// 	},
			// };

			for faucet_data in faucet_datas{
				if faucet_data.id != latest_id {
					let param_created_at_str = str::from_utf8(&faucet_data.created_at).map_err(|_| Error::<T>::StrConvertError)?;
					let created_at_for_param_data = DateTime::parse_from_rfc3339(param_created_at_str).unwrap();
					let created_at_for_latest_data = DateTime::parse_from_rfc3339(latest_created_at_str).unwrap();
					let duration: Duration = created_at_for_param_data - created_at_for_latest_data;
					if duration.num_seconds() >= 0 {
						let to_address = Self::convert_vec_to_accountid(faucet_data.address.clone());
						let token_amoount : Balance<T> = TOKEN_AMOUNT.saturated_into();
						// debug::info!("##### from:{:#?}, to:{:#?}, amount:{:#?}",who,to_address,token_amoount);
						match T::Currency::transfer(&who, &to_address, token_amoount, ExistenceRequirement::KeepAlive){
//							Ok(())=> debug::info!("faucet transfer token is succeed. address is : {:#?}", to_address),
							Ok(())=> log::info!("faucet transfer token is succeed. address is : {:#?}", to_address),
							Err(e)=> log::error!("##### transfer token is failed. : {:#?}", e), 
						};
						// debug::info!("##### update LatestFaucetData.");
						LatestFaucetData::<T>::put(faucet_data);
						<Sendlist<T>>::insert(to_address.clone(), <frame_system::Pallet<T>>::block_number());
						Self::deposit_event(Event::TestNetTokenTransfered(to_address.clone()));
					}
				}
			}
			Ok(())
		}

	}
}

#[serde(crate = "alt_serde")]
#[derive(Deserialize, Encode, Decode, Default)]
struct User{
	#[serde(deserialize_with = "de_string_to_bytes")]
	login: Vec<u8>,
	id: u64,
	#[serde(deserialize_with = "de_string_to_bytes")]
	node_id: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	avatar_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	gravatar_id: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	html_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	followers_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	following_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	gists_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	starred_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	subscriptions_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	organizations_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	repos_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	events_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	received_events_url: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	r#type: Vec<u8>,
	site_admin: bool,
}

#[serde(crate = "alt_serde")]
#[derive(Deserialize, Encode, Decode, Default)]
struct GithubInfo {
	#[serde(deserialize_with = "de_string_to_bytes")]
	url: Vec<u8>,
	id: u64,
	#[serde(deserialize_with = "de_string_to_bytes")]
	node_id: Vec<u8>,
	user: User,
	#[serde(deserialize_with = "de_string_to_bytes")]
	created_at: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	updated_at: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	body: Vec<u8>,
}

pub fn de_string_to_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
where
	D: Deserializer<'de>,
{
	let s: &str = Deserialize::deserialize(de)?;
	Ok(s.as_bytes().to_vec())
}

impl fmt::Debug for GithubInfo {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"{{ url: {}, id: {}, node_id: {}, login: {}, created_at: {}, updated_at: {}, body: {} }}",
			str::from_utf8(&self.url).map_err(|_| fmt::Error)?,
			self.id,
			str::from_utf8(&self.node_id).map_err(|_| fmt::Error)?,
			str::from_utf8(&self.user.login).map_err(|_| fmt::Error)?,
			str::from_utf8(&self.created_at).map_err(|_| fmt::Error)?,
			str::from_utf8(&self.updated_at).map_err(|_| fmt::Error)?,
			str::from_utf8(&self.body).map_err(|_| fmt::Error)?,
		)
	}
}

impl<T: Config> Pallet<T> {
	// fn check_faucet_datas(latest_faucet_data:Option<FaucetData>){
	fn check_faucet_datas(latest_faucet_data:FaucetData){
			let now = sp_io::offchain::timestamp().unix_millis();
		let last_check_time = StorageValueRef::persistent(b"offchain-test::last_check_time");
		if let Ok(data) = last_check_time.get::<u64>() {
			// debug::info!("##### last_check_time: {:?}", last_check_time);
			let checktime = match data {
				Some(res)=> res,
				None=>{
					log::error!("getting last_chek_time is error");
					return;
				},
			};
			if now - checktime < FAUCET_CHECK_INTERVAL{
				return;
			}
		}
		
		// debug::info!("##### faucet executed");
		last_check_time.set(&now);
		let g_infos = match Self::fetch_data(){
			Ok(res)=>res,
			Err(e)=>{
				log::error!("Off-chain Faucet Error fetch_data: {}", e);
				return
			}
		};
		if g_infos.len() > 0 {
			let target_faucet_datas = match Self::check_fetch_data(g_infos,latest_faucet_data){
				Ok(res)=>res,
				Err(e)=>{
					log::error!("Off-chain Faucet Error check_data: {}", e);
					return
				}
			};
			if target_faucet_datas.len() > 0 {
				// debug::info!("##### transaction will be executed.");
				match Self::offchain_signed_tx(target_faucet_datas) {
					Ok(res)=> res,
					Err(e)=> log::error!("Off-chain Faucet offchain_signed_tx is failed.: {:#?}", e),
				}
			}
		}
	}

	fn check_fetch_data(g_infos: Vec<GithubInfo>, latest_faucet_data:FaucetData) -> Result<Vec<FaucetData>, &'static str>{
		let mut results = Vec::new();
		let tmp = latest_faucet_data.created_at;
		let latest_created_at_str = str::from_utf8(&tmp).map_err(|_| Error::<T>::StrConvertError)?;
		let latest_id = latest_faucet_data.id;
		// match latest_faucet_data{
		// 	Some(data)=> {
		// 		tmp = data.created_at;
		// 		latest_created_at_str = str::from_utf8(&tmp).map_err(|_| Error::<T>::StrConvertError)?;
		// 		latest_id = data.id;
		// 	},
		// 	None=>{
		// 		latest_created_at_str = "1976-09-24T16:00:00Z";
		// 		latest_id = 0;
		// 	},
		// };
		for g_info in g_infos{
			if g_info.id != latest_id {
				let param_created_at_str = str::from_utf8(&g_info.created_at).map_err(|_| Error::<T>::StrConvertError)?;
				let created_at_for_param_data = DateTime::parse_from_rfc3339(param_created_at_str).unwrap();
				let created_at_for_latest_data = DateTime::parse_from_rfc3339(latest_created_at_str).unwrap();
				let duration: Duration = created_at_for_param_data - created_at_for_latest_data;
				if duration.num_seconds() >= 0 {
					let body_str = str::from_utf8(&g_info.body).map_err(|_| Error::<T>::StrConvertError)?;
					let body_value: Vec<&str> = body_str.split("<br>").collect();
					let mut address_str = "";
					let mut homework_str = "";
					for key_value_str in body_value{
						let key_value: Vec<&str> = key_value_str.split(':').collect();
						if key_value.len() != 2{
							log::warn!("##### This is not key_value string: {:#?}", key_value);
							continue;
						}
						if key_value_str.find("address") >= Some(0){
							address_str = key_value[1];
						}
						if key_value_str.find("homework") >= Some(0){
							homework_str = key_value[1];
						}
					}
					let mut output = vec![0xFF; 35];
					match bs58::decode(address_str).into(&mut output){
						Ok(_res)=> (), // debug::warn!("##### bs58.decode is succeed."),
						Err(_e)=> {
							log::warn!("##### This is invalid address : {:#?} : {:#?}", address_str, _e);
							continue;
						},
					};
					let cut_address_vec:Vec<_> = output.drain(1..33).collect();
					let homework_vec:Vec<_> = homework_str.from_hex().unwrap();
					if cut_address_vec != homework_vec {
						log::warn!("##### This is invalid address or invalid homework: {:#?}", homework_str);
						continue;
					}
					let to_address_vec:Vec<u8> = homework_str.from_hex().unwrap();
					let to_address = Self::convert_vec_to_accountid(to_address_vec.clone());
					match <Sendlist<T>>::get(to_address.clone()) {
						Some(result) => {
							let block_number = result + WAIT_BLOCK_NUMBER.into();
							if block_number > <frame_system::Pallet<T>>::block_number() {
								return Err(Error::<T>::TimeHasNotPassed)?;
							}
							else{
								<Sendlist<T>>::remove(to_address.clone());
							}
						},
						None => (),
					};
					let result = FaucetData{
						id: g_info.id,
						login: g_info.user.login,
						created_at: g_info.created_at,
						address: to_address_vec.clone(),
					};
					results.push(result);
				}
			}
		}
		Ok(results)
	}

	fn fetch_data() -> Result<Vec<GithubInfo>, &'static str> {
		let pending = http::Request::get(HTTP_REMOTE_REQUEST)
			.add_header("User-Agent", HTTP_HEADER_USER_AGENT)
			.add_header("Accept-Charset", "UTF-8")
			.send()
			.map_err(|_| "Error in sending http GET request")?;
		let response = pending.wait()
			.map_err(|_| "Error in waiting http response back")?;

		if response.code != 200 {
			log::warn!("Unexpected status code: {}", response.code);
			return Err(<Error<T>>::HttpFetchingError)?;
		} 
		let resp_bytes = response.body().collect::<Vec<u8>>();
		let resp_str = str::from_utf8(&resp_bytes).map_err(|_| <Error<T>>::HttpFetchingError)?;
		let resp_str2 = resp_str.replace(r"\r\n","<br>");
		let gh_info: Vec<GithubInfo> =
			serde_json::from_str(&resp_str2).map_err(|_| <Error<T>>::HttpFetchingError)?;
		Ok(gh_info)
	}

	fn offchain_signed_tx(faucet_datas: Vec<FaucetData>) -> Result<(), Error<T>> {
		let signer = Signer::<T, T::AuthorityId>::any_account();
		let result = signer.send_signed_transaction(|_acct|
			Call::send_some_testnet_token(faucet_datas.clone())
		);

		if let Some((_acc, res)) = result {
			if res.is_err() {
				log::error!("Execute transaction is failed.AccountId is {:#?}",_acc.id);
				return Err(<Error<T>>::OffchainSignedTxError);
			}
			return Ok(());
		}
		// debug::error!("No local account available");
		Err(<Error<T>>::NoLocalAcctForSigning)
	}

	fn convert_vec_to_accountid(account_vec: Vec<u8>)-> <T as system::Config>::AccountId{
		let mut array = [0; 32];
		let bytes = &account_vec[..array.len()]; 
		array.copy_from_slice(bytes); 
		let account32: AccountId32 = array.into();
		let mut to32 = AccountId32::as_ref(&account32);
		let to_address : T::AccountId = T::AccountId::decode(&mut to32).unwrap_or_default();
		to_address
	}
}