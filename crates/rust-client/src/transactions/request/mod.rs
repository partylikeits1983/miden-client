//! Contains structures and functions related to transaction creation.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

use miden_objects::{
    accounts::AccountId,
    assembly::AssemblyError,
    crypto::merkle::MerkleStore,
    notes::{Note, NoteDetails, NoteId, NoteTag, PartialNote},
    transaction::{TransactionArgs, TransactionScript},
    vm::AdviceMap,
    Digest, Felt, NoteError, Word,
};
use miden_tx::utils::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable};
use thiserror::Error;

use super::{
    script_builder::{AccountCapabilities, TransactionScriptBuilder},
    TransactionScriptBuilderError,
};

mod builder;
pub use builder::{PaymentTransactionData, SwapTransactionData, TransactionRequestBuilder};

mod foreign;
pub use foreign::{ForeignAccount, ForeignAccountInputs};

// TRANSACTION REQUEST
// ================================================================================================

pub type NoteArgs = Word;

/// Specifies a transaction script to be executed in a transaction.
///
/// A transaction script is a program which is executed after scripts of all input notes have been
/// executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionScriptTemplate {
    /// Specifies the exact transaction script to be executed in a transaction.
    CustomScript(TransactionScript),
    /// Specifies that the transaction script must create the specified output notes.
    ///
    /// It is up to the client to determine how the output notes will be created and this will
    /// depend on the capabilities of the account the transaction request will be applied to.
    /// For example, for Basic Wallets, this may involve invoking `create_note` procedure.
    SendNotes(Vec<PartialNote>),
}

/// Specifies a transaction request that can be executed by an account.
///
/// A request contains information about input notes to be consumed by the transaction (if any),
/// description of the transaction script to be executed (if any), and a set of notes expected
/// to be generated by the transaction or by consuming notes generated by the transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionRequest {
    /// Notes to be consumed by the transaction that aren't authenticated.
    unauthenticated_input_notes: Vec<Note>,
    /// Notes to be consumed by the transaction together with their (optional) arguments. This
    /// includes both authenticated and unauthenticated notes.
    input_notes: BTreeMap<NoteId, Option<NoteArgs>>,
    /// Template for the creation of the transaction script.
    script_template: Option<TransactionScriptTemplate>,
    /// A map of notes expected to be generated by the transactions.
    expected_output_notes: BTreeMap<NoteId, Note>,
    /// A map of details and tags of notes we expect to be created as part of future transactions
    /// with their respective tags.
    ///
    /// For example, after a swap note is consumed, a payback note is expected to be created.
    expected_future_notes: BTreeMap<NoteId, (NoteDetails, NoteTag)>,
    /// Initial state of the `AdviceMap` that provides data during runtime.
    advice_map: AdviceMap,
    /// Initial state of the `MerkleStore` that provides data during runtime.
    merkle_store: MerkleStore,
    /// Foreign account data requirements. At execution time, account data will be retrieved from
    /// the network, and injected as advice inputs. Additionally, the account's code will be
    /// added to the executor and prover.
    foreign_accounts: BTreeSet<ForeignAccount>,
    /// The number of blocks in relation to the transaction's reference block after which the
    /// transaction will expire.
    expiration_delta: Option<u16>,
}

impl TransactionRequest {
    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the transaction request's unauthenticated note list.
    pub fn unauthenticated_input_notes(&self) -> &[Note] {
        &self.unauthenticated_input_notes
    }

    /// Returns an iterator over unauthenticated note IDs for the transaction request.
    pub fn unauthenticated_input_note_ids(&self) -> impl Iterator<Item = NoteId> + '_ {
        self.unauthenticated_input_notes.iter().map(|note| note.id())
    }

    /// Returns an iterator over authenticated input note IDs for the transaction request.
    pub fn authenticated_input_note_ids(&self) -> impl Iterator<Item = NoteId> + '_ {
        let unauthenticated_note_ids: BTreeSet<NoteId> =
            BTreeSet::from_iter(self.unauthenticated_input_note_ids());

        self.input_notes()
            .iter()
            .map(|(note_id, _)| *note_id)
            .filter(move |note_id| !unauthenticated_note_ids.contains(note_id))
    }

    /// Returns a mapping for input note IDs and their optional [NoteArgs].
    pub fn input_notes(&self) -> &BTreeMap<NoteId, Option<NoteArgs>> {
        &self.input_notes
    }

    /// Returns a list of all input note IDs.
    pub fn get_input_note_ids(&self) -> Vec<NoteId> {
        self.input_notes.keys().cloned().collect()
    }

    /// Returns a map of note IDs to their respective [NoteArgs]. The result will include
    /// exclusively note IDs for notes for which [NoteArgs] have been defined.
    pub fn get_note_args(&self) -> BTreeMap<NoteId, NoteArgs> {
        self.input_notes
            .iter()
            .filter_map(|(note, args)| args.map(|a| (*note, a)))
            .collect()
    }

    /// Returns an iterator over the expected output notes.
    pub fn expected_output_notes(&self) -> impl Iterator<Item = &Note> {
        self.expected_output_notes.values()
    }

    /// Returns an iterator over expected future notes.
    pub fn expected_future_notes(&self) -> impl Iterator<Item = &(NoteDetails, NoteTag)> {
        self.expected_future_notes.values()
    }

    /// Returns the [TransactionScriptTemplate].
    pub fn script_template(&self) -> &Option<TransactionScriptTemplate> {
        &self.script_template
    }

    /// Returns the [AdviceMap] for the transaction request.
    pub fn advice_map(&self) -> &AdviceMap {
        &self.advice_map
    }

    /// Returns the [MerkleStore] for the transaction request.
    pub fn merkle_store(&self) -> &MerkleStore {
        &self.merkle_store
    }

    /// Returns the IDs of the required foreign accounts for the transaction request.
    pub fn foreign_accounts(&self) -> &BTreeSet<ForeignAccount> {
        &self.foreign_accounts
    }

    /// Converts the [TransactionRequest] into [TransactionArgs] in order to be executed by a Miden
    /// host.
    pub(super) fn into_transaction_args(self, tx_script: TransactionScript) -> TransactionArgs {
        let note_args = self.get_note_args();
        let TransactionRequest {
            expected_output_notes,
            advice_map,
            merkle_store,
            ..
        } = self;

        let mut tx_args = TransactionArgs::new(Some(tx_script), note_args.into(), advice_map);

        tx_args.extend_expected_output_notes(expected_output_notes.into_values());
        tx_args.extend_merkle_store(merkle_store.inner_nodes());

        tx_args
    }

    pub(crate) fn build_transaction_script(
        &self,
        account_capabilities: AccountCapabilities,
    ) -> Result<TransactionScript, TransactionRequestError> {
        match &self.script_template {
            Some(TransactionScriptTemplate::CustomScript(script)) => Ok(script.clone()),
            Some(TransactionScriptTemplate::SendNotes(notes)) => {
                let tx_script_builder =
                    TransactionScriptBuilder::new(account_capabilities, self.expiration_delta);

                Ok(tx_script_builder.build_send_notes_script(notes)?)
            },
            None => {
                if self.input_notes.is_empty() {
                    Err(TransactionRequestError::NoInputNotes)
                } else {
                    let tx_script_builder =
                        TransactionScriptBuilder::new(account_capabilities, self.expiration_delta);

                    Ok(tx_script_builder.build_auth_script()?)
                }
            },
        }
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for TransactionRequest {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.unauthenticated_input_notes.write_into(target);
        self.input_notes.write_into(target);
        match &self.script_template {
            None => target.write_u8(0),
            Some(TransactionScriptTemplate::CustomScript(script)) => {
                target.write_u8(1);
                script.write_into(target);
            },
            Some(TransactionScriptTemplate::SendNotes(notes)) => {
                target.write_u8(2);
                notes.write_into(target);
            },
        }
        self.expected_output_notes.write_into(target);
        self.expected_future_notes.write_into(target);
        self.advice_map.clone().into_iter().collect::<Vec<_>>().write_into(target);
        self.merkle_store.write_into(target);
        self.foreign_accounts.write_into(target);
        self.expiration_delta.write_into(target);
    }
}

impl Deserializable for TransactionRequest {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let unauthenticated_input_notes = Vec::<Note>::read_from(source)?;
        let input_notes = BTreeMap::<NoteId, Option<NoteArgs>>::read_from(source)?;

        let script_template = match source.read_u8()? {
            0 => None,
            1 => {
                let transaction_script = TransactionScript::read_from(source)?;
                Some(TransactionScriptTemplate::CustomScript(transaction_script))
            },
            2 => {
                let notes = Vec::<PartialNote>::read_from(source)?;
                Some(TransactionScriptTemplate::SendNotes(notes))
            },
            _ => {
                return Err(DeserializationError::InvalidValue(
                    "Invalid script template type".to_string(),
                ))
            },
        };

        let expected_output_notes = BTreeMap::<NoteId, Note>::read_from(source)?;
        let expected_future_notes = BTreeMap::<NoteId, (NoteDetails, NoteTag)>::read_from(source)?;

        let mut advice_map = AdviceMap::new();
        let advice_vec = Vec::<(Digest, Vec<Felt>)>::read_from(source)?;
        advice_map.extend(advice_vec);
        let merkle_store = MerkleStore::read_from(source)?;
        let foreign_accounts = BTreeSet::<ForeignAccount>::read_from(source)?;
        let expiration_delta = Option::<u16>::read_from(source)?;

        Ok(TransactionRequest {
            unauthenticated_input_notes,
            input_notes,
            script_template,
            expected_output_notes,
            expected_future_notes,
            advice_map,
            merkle_store,
            expiration_delta,
            foreign_accounts,
        })
    }
}

impl Default for TransactionRequestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// TRANSACTION REQUEST ERROR
// ================================================================================================

// Errors related to a [TransactionRequest]
#[derive(Debug, Error)]
pub enum TransactionRequestError {
    #[error("foreign account data missing in the account proof")]
    ForeignAccountDataMissing,
    #[error("requested foreign account with ID {0} does not have an expected storage mode")]
    InvalidForeignAccountId(AccountId),
    #[error("every authenticated note to be consumed should be committed and contain a valid inclusion proof")]
    InputNoteNotAuthenticated,
    #[error(
        "the input notes map should include keys for all provided unauthenticated input notes"
    )]
    InputNotesMapMissingUnauthenticatedNotes,
    #[error("own notes shouldn't be of the header variant")]
    InvalidNoteVariant,
    #[error("invalid sender account id: {0}")]
    InvalidSenderAccount(AccountId),
    #[error("invalid transaction script")]
    //TODO: use source in this error when possible
    InvalidTransactionScript(AssemblyError),
    #[error("a transaction without output notes must have at least one input note")]
    NoInputNotes,
    #[error("transaction script template error: {0}")]
    ScriptTemplateError(String),
    #[error("note not found: {0}")]
    NoteNotFound(String),
    #[error("note creation error")]
    NoteCreationError(#[from] NoteError),
    #[error("transaction script builder error")]
    TransactionScriptBuilderError(#[from] TransactionScriptBuilderError),
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use miden_lib::{notes::create_p2id_note, transaction::TransactionKernel};
    use miden_objects::{
        accounts::{AccountBuilder, AccountId, AccountType},
        assets::FungibleAsset,
        crypto::rand::{FeltRng, RpoRandomCoin},
        notes::{NoteExecutionMode, NoteTag, NoteType},
        testing::account_component::AccountMockComponent,
        transaction::OutputNote,
        Digest, Felt, ZERO,
    };
    use miden_tx::utils::{Deserializable, Serializable};

    use super::{TransactionRequest, TransactionRequestBuilder};

    #[test]
    fn transaction_request_serialization() {
        let sender_id = AccountId::new_dummy([0u8; 32], AccountType::RegularAccountImmutableCode);
        let target_id = AccountId::new_dummy([1u8; 32], AccountType::RegularAccountImmutableCode);
        let faucet_id = AccountId::new_dummy([2u8; 32], AccountType::FungibleFaucet);
        let mut rng = RpoRandomCoin::new(Default::default());

        let mut notes = vec![];
        for i in 0..6 {
            let note = create_p2id_note(
                sender_id,
                target_id,
                vec![FungibleAsset::new(faucet_id, 100 + i).unwrap().into()],
                NoteType::Private,
                ZERO,
                &mut rng,
            )
            .unwrap();
            notes.push(note);
        }

        let mut advice_vec: Vec<(Digest, Vec<Felt>)> = vec![];
        for i in 0..10 {
            advice_vec.push((Digest::new(rng.draw_word()), vec![Felt::new(i)]));
        }

        let account = AccountBuilder::new()
            .init_seed(Default::default())
            .with_component(
                AccountMockComponent::new_with_empty_slots(TransactionKernel::assembler()).unwrap(),
            )
            .account_type(AccountType::RegularAccountImmutableCode)
            .storage_mode(miden_objects::accounts::AccountStorageMode::Private)
            .build_existing()
            .unwrap();

        // This transaction request wouldn't be valid in a real scenario, it's intended for testing
        let tx_request = TransactionRequestBuilder::new()
            .with_authenticated_input_notes(vec![(notes.pop().unwrap().id(), None)])
            .with_unauthenticated_input_notes(vec![(notes.pop().unwrap(), None)])
            .with_expected_output_notes(vec![notes.pop().unwrap()])
            .with_expected_future_notes(vec![(
                notes.pop().unwrap().into(),
                NoteTag::from_account_id(sender_id, NoteExecutionMode::Local).unwrap(),
            )])
            .extend_advice_map(advice_vec)
            .with_public_foreign_accounts([target_id])
            .unwrap()
            .with_private_foreign_accounts([account])
            .unwrap()
            .with_own_output_notes(vec![
                OutputNote::Full(notes.pop().unwrap()),
                OutputNote::Partial(notes.pop().unwrap().into()),
            ])
            .unwrap()
            .build();

        let mut buffer = Vec::new();
        tx_request.write_into(&mut buffer);

        let deserialized_tx_request = TransactionRequest::read_from_bytes(&buffer).unwrap();
        assert_eq!(tx_request, deserialized_tx_request);
    }
}
