#![cfg_attr(not(feature = "std"), no_std)]

mod source;
mod rates;

use source::{Source, get_rand_source};
use rates::{Rates, parse_json};

use codec::{Decode, Encode};
use frame_support::traits::Get;
use frame_system::{
	self as system,
	offchain::{
		AppCrypto, CreateSignedTransaction, SendSignedTransaction,
		SignedPayload, Signer, SigningTypes, SubmitTransaction,
	},
};
use sp_core::crypto::KeyTypeId;
use sp_runtime::{
	offchain::{
		http,
		Duration,
		storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
	},
	transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
	RuntimeDebug,
};
use sp_std::vec::Vec;

/// Defines application identifier for crypto keys of this module.
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"exc!");

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrappers.
pub mod crypto {
	use super::KEY_TYPE;
	use sp_core::sr25519::Signature as Sr25519Signature;
	use sp_runtime::{
		app_crypto::{app_crypto, sr25519},
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

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// This pallet's configuration trait
	#[pallet::config]
	pub trait Config: CreateSignedTransaction<Call<Self>> + frame_system::Config {
		/// The identifier type for an offchain worker.
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		// Configuration parameters

		/// A grace period after we send transaction.
		#[pallet::constant]
		type GracePeriod: Get<Self::BlockNumber>;

		/// Number of blocks of cooldown after unsigned transaction is included.
		#[pallet::constant]
		type UnsignedInterval: Get<Self::BlockNumber>;

		/// Maximum number of prices.
		#[pallet::constant]
		type MaxPrices: Get<u32>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Offchain Worker entry point.
		fn offchain_worker(block_number: T::BlockNumber) {
			log::info!("Hello World from offchain workers!");

			let parent_hash = <system::Pallet<T>>::block_hash(block_number - 1u32.into());
			log::debug!("Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

			let average: Option<u32> = Self::average_price();
			log::debug!("Current price: {:?}", average);

			//
			let b = "USD".to_owned();
			let q = "EUR".to_owned();
			let vals: Vec<String> = vec![b.clone(), q.clone()];
			//

			let res = Self::fetch_price_and_send_signed(vals);

			if let Err(e) = res {
				log::error!("Error: {}", e);
			}
		}
	}

	/// A public part of the pallet.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit new price to the list.
		#[pallet::call_index(0)]
		#[pallet::weight({0})]
		pub fn submit_price(origin: OriginFor<T>, price: u32) -> DispatchResultWithPostInfo {
			// Retrieve sender of the transaction.
			let who = ensure_signed(origin)?;
			// Add the price to the on-chain list.
			Self::add_price(Some(who), price);
			Ok(().into())
		}
	}

	/// Events for the pallet.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event generated when new price is accepted to contribute to the average.
		NewPrice { price: u32, maybe_who: Option<T::AccountId> },
	}

	/// A vector of recently submitted prices.
	#[pallet::storage]
	#[pallet::getter(fn prices)]
	pub(super) type Prices<T: Config> = StorageValue<_, BoundedVec<u32, T::MaxPrices>, ValueQuery>;
}

/// Payload used by this crate to hold price data required to submit a transaction.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, scale_info::TypeInfo)]
pub struct PricePayload<Public, BlockNumber> {
	block_number: BlockNumber,
	price: u32,
	public: Public,
}

impl<T: SigningTypes> SignedPayload<T> for PricePayload<T::Public, T::BlockNumber> {
	fn public(&self) -> T::Public {
		self.public.clone()
	}
}

impl<T: Config> Pallet<T> {	
	/// A helper function to fetch the price and send signed transaction.
	fn fetch_price_and_send_signed(tickers: Vec<String>) -> Result<(), &'static str> {
		let signer = Signer::<T, T::AuthorityId>::all_accounts();
		if !signer.can_sign() {
			return Err(
				"No local accounts available. Consider adding one via `author_insertKey` RPC.",
			)
		}
		// Make an external HTTP request to fetch the current price.
		let price = Self::fetch_price_simple(tickers).map_err(|_| "Failed to fetch price")?;

		let results = signer.send_signed_transaction(|_account| {
			Call::submit_price { price }
		});

		for (acc, res) in &results {
			match res {
				Ok(()) => log::info!("[{:?}] Submitted price of {} cents", acc.id, price),
				Err(e) => log::error!("[{:?}] Failed to submit transaction: {:?}", acc.id, e),
			}
		}

		Ok(())
	}

	/// Makes simple request, fetch current price.
	fn fetch_price_simple(tickers: Vec<String>) -> Result<u32, http::Error> {
		let timeout = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));

		let source: Source = get_rand_source(tickers.clone());

		let request = http::Request::get(source.url());

		let pending = request.deadline(timeout)
			.send()
			.map_err(|_| http::Error::IoError)?;
		let response = pending.try_wait(timeout)
			.map_err(|_| http::Error::DeadlineReached)??;

		if response.code != 200 {
			log::warn!("Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown)
		}

		let body = response.body().collect::<Vec<u8>>();

		let body_str = sp_std::str::from_utf8(&body).map_err(|_| {
				log::warn!("Failed to parse reponse body to utf8 str");
				http::Error::Unknown
		})?;

		let rates: Rates = parse_json(body_str);
		let price =  rates.rate(&tickers[1]);

		Ok(price)
	}

	/// Makes full request to user's api, fetch current price.  
	// fn fetch_price_full(base: String, path: String, query: Vec<(String, String)>,
	// 	headers: HeaderMap) -> Result<f64, Box<dyn std::error::Error>> {

	// }

	/// Add new price to the list.
	fn add_price(maybe_who: Option<T::AccountId>, price: u32) {
		log::info!("Adding to the average: {}", price);

		<Prices<T>>::mutate(|prices| {
			if prices.try_push(price).is_err() {
				prices[(price % T::MaxPrices::get()) as usize] = price;
			}
		});

		let average = Self::average_price()
			.expect("The average is not empty, because it was just mutated; qed");
		log::info!("Current average price is: {}", average);

		Self::deposit_event(Event::NewPrice { price, maybe_who });
	}

	/// Calculate current average price.
	fn average_price() -> Option<u32> {
		let prices = <Prices<T>>::get();
		if prices.is_empty() {
			None
		} else {
			Some(prices.iter().fold(0_u32, |a, b| a.saturating_add(*b)) / prices.len() as u32)
		}
	}
}
