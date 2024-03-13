use crate::errors::StoreError;
use miden_objects::{
    crypto::merkle::{InOrderIndex, MmrPeaks},
    notes::{NoteId, NoteInclusionProof},
    transaction::TransactionId,
    BlockHeader, Digest,
};
use rusqlite::{named_params, params};

use super::SqliteStore;

impl SqliteStore {
    pub(crate) fn get_note_tags(&self) -> Result<Vec<u64>, StoreError> {
        const QUERY: &str = "SELECT tags FROM state_sync";

        self.db
            .prepare(QUERY)?
            .query_map([], |row| row.get(0))
            .expect("no binding parameters used in query")
            .map(|result| {
                result
                    .map_err(|err| StoreError::ParsingError(err.to_string()))
                    .and_then(|v: String| {
                        serde_json::from_str(&v).map_err(StoreError::JsonDataDeserializationError)
                    })
            })
            .next()
            .expect("state sync tags exist")
    }

    pub(super) fn add_note_tag(&mut self, tag: u64) -> Result<bool, StoreError> {
        let mut tags = self.get_note_tags()?;
        if tags.contains(&tag) {
            return Ok(false);
        }
        tags.push(tag);
        let tags = serde_json::to_string(&tags).map_err(StoreError::InputSerializationError)?;

        const QUERY: &str = "UPDATE state_sync SET tags = ?";
        self.db.execute(QUERY, params![tags])?;

        Ok(true)
    }

    pub(super) fn get_sync_height(&self) -> Result<u32, StoreError> {
        const QUERY: &str = "SELECT block_num FROM state_sync";

        self.db
            .prepare(QUERY)?
            .query_map([], |row| row.get(0))
            .expect("no binding parameters used in query")
            .map(|result| Ok(result?).map(|v: i64| v as u32))
            .next()
            .expect("state sync block number exists")
    }

    pub(super) fn apply_state_sync(
        &mut self,
        block_header: BlockHeader,
        nullifiers: Vec<Digest>,
        committed_notes: Vec<(NoteId, NoteInclusionProof)>,
        committed_transactions: &[TransactionId],
        new_mmr_peaks: MmrPeaks,
        new_authentication_nodes: &[(InOrderIndex, Digest)],
    ) -> Result<(), StoreError> {
        let tx = self.db.transaction()?;

        // Update state sync block number
        const BLOCK_NUMBER_QUERY: &str = "UPDATE state_sync SET block_num = ?";
        tx.execute(BLOCK_NUMBER_QUERY, params![block_header.block_num()])?;

        // Update spent notes
        for nullifier in nullifiers.iter() {
            const SPENT_INPUT_NOTE_QUERY: &str =
                "UPDATE input_notes SET status = 'consumed' WHERE json_extract(details, '$.nullifier') = ?";
            let nullifier = nullifier.to_hex();
            tx.execute(SPENT_INPUT_NOTE_QUERY, params![nullifier])?;

            const SPENT_OUTPUT_NOTE_QUERY: &str =
                "UPDATE output_notes SET status = 'consumed' WHERE json_extract(details, '$.nullifier') = ?";
            tx.execute(SPENT_OUTPUT_NOTE_QUERY, params![nullifier])?;
        }

        // TODO: Due to the fact that notes are returned based on fuzzy matching of tags,
        // this process of marking if the header has notes needs to be revisited
        let block_has_relevant_notes = !committed_notes.is_empty();
        Self::insert_block_header_tx(&tx, block_header, new_mmr_peaks, block_has_relevant_notes)?;

        // Insert new authentication nodes (inner nodes of the PartialMmr)
        Self::insert_chain_mmr_nodes(&tx, new_authentication_nodes)?;

        // Update tracked notes
        for (note_id, inclusion_proof) in committed_notes.iter() {
            let block_num = inclusion_proof.origin().block_num;
            let sub_hash = inclusion_proof.sub_hash();
            let note_root = inclusion_proof.note_root();
            let note_index = inclusion_proof.origin().node_index.value();

            let inclusion_proof = serde_json::to_string(&NoteInclusionProof::new(
                block_num,
                sub_hash,
                note_root,
                note_index,
                inclusion_proof.note_path().clone(),
            )?)
            .map_err(StoreError::InputSerializationError)?;

            const COMMITTED_INPUT_NOTES_QUERY: &str =
                "UPDATE input_notes SET status = 'committed', inclusion_proof = json(:inclusion_proof) WHERE note_id = :note_id";

            tx.execute(
                COMMITTED_INPUT_NOTES_QUERY,
                named_params! {
                    ":inclusion_proof": inclusion_proof,
                    ":note_id": note_id.inner().to_hex()
                },
            )?;

            // Update output notes
            const COMMITTED_OUTPUT_NOTES_QUERY: &str =
                "UPDATE output_notes SET status = 'committed', inclusion_proof = json(:inclusion_proof) WHERE note_id = :note_id";

            tx.execute(
                COMMITTED_OUTPUT_NOTES_QUERY,
                named_params! {
                    ":inclusion_proof": inclusion_proof,
                    ":note_id": note_id.inner().to_hex()
                },
            )?;
        }

        // Mark transactions as committed
        Self::mark_transactions_as_committed(
            &tx,
            block_header.block_num(),
            committed_transactions,
        )?;

        // Commit the updates
        tx.commit()?;

        Ok(())
    }
}
