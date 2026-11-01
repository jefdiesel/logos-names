//! Names carried over from an earlier chain. A snapshot of that chain's registry
//! lists every account it had; the claims list is one leaf per record, sorted by
//! address, and an admin and every claimant build it the same way from the same file.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use borsh::BorshDeserialize;
use lee_core::account::AccountId;
use names_core::{ClaimProof, NameId, OwnerTag, Settings, claim_leaf, claim_proof, claim_root, page_address, record_address};

/// The layout version of records and pages on the earlier chain.
const OLD_VERSION: u8 = 5;

/// A record on the earlier chain, which had no `claimed` flag.
#[derive(Debug, Clone, PartialEq, Eq, BorshDeserialize)]
pub struct OldRecord {
    pub version: u8,
    pub owner: OwnerTag,
    pub expires: u64,
    pub sealed: Vec<u8>,
}

#[derive(BorshDeserialize)]
struct OldPage {
    version: u8,
    sealed: Vec<u8>,
}

#[derive(serde::Deserialize)]
struct File {
    program: String,
    height: u64,
    accounts: Vec<Entry>,
}

#[derive(serde::Deserialize)]
struct Entry {
    account: String,
    data: String,
}

pub struct Snapshot {
    pub program: AccountId,
    pub height: u64,
    accounts: Vec<(AccountId, Vec<u8>)>,
}

impl Snapshot {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        let file: File = serde_json::from_slice(&text).with_context(|| format!("{} is not a snapshot", path.display()))?;
        Self::parse(file)
    }

    fn parse(file: File) -> Result<Self> {
        let program = file.program.parse().map_err(|_| anyhow!("the snapshot's program is not an account ID"))?;
        let accounts = file
            .accounts
            .into_iter()
            .map(|entry| {
                let account = entry.account.parse().map_err(|_| anyhow!("{} is not an account ID", entry.account))?;
                let data = hex::decode(&entry.data).with_context(|| format!("the data of {} is not hex", entry.account))?;
                Ok((account, data))
            })
            .collect::<Result<_>>()?;
        Ok(Self { program, height: file.height, accounts })
    }

    /// Every record: the snapshot also holds settings, commitments and pages, whose
    /// lengths and layouts do not decode as one.
    fn records(&self) -> Vec<(AccountId, OldRecord)> {
        let mut records: Vec<(AccountId, OldRecord)> = self
            .accounts
            .iter()
            .filter_map(|(account, data)| {
                let record: OldRecord = borsh::from_slice(data).ok()?;
                (record.version == OLD_VERSION).then_some((*account, record))
            })
            .collect();
        records.sort_by_key(|(account, _)| *account.value());
        records
    }

    /// The claims list, in its order.
    #[must_use]
    pub fn leaves(&self) -> Vec<[u8; 32]> {
        self.records().iter().map(|(account, record)| claim_leaf(account, &record.owner)).collect()
    }

    #[must_use]
    pub fn root(&self) -> [u8; 32] {
        claim_root(&self.leaves())
    }

    /// The proof for the name `id`, and its record.
    #[must_use]
    pub fn claim(&self, id: &NameId) -> Option<(ClaimProof, OldRecord)> {
        let address = record_address(&self.program, id);
        let records = self.records();
        let index = records.iter().position(|(account, _)| *account == address)?;
        Some((claim_proof(&self.leaves(), index), records[index].1.clone()))
    }

    /// The sealed bytes of the name's page, if it had one.
    #[must_use]
    pub fn page(&self, id: &NameId) -> Option<Vec<u8>> {
        let address = page_address(&self.program, id);
        let (_, data) = self.accounts.iter().find(|(account, _)| *account == address)?;
        let page: OldPage = borsh::from_slice(data).ok()?;
        (page.version == OLD_VERSION && !page.sealed.is_empty()).then_some(page.sealed)
    }
}

/// The claim settings `configure` was asked for; what isn't given stays as it is.
pub struct ClaimSettings {
    list: Option<([u8; 32], AccountId)>,
    until: Option<u64>,
}

/// The settings' claim fields to write.
pub struct Claims {
    pub root: [u8; 32],
    pub until: u64,
    pub program: AccountId,
}

impl ClaimSettings {
    pub fn from_args(
        snapshot: Option<PathBuf>,
        root: Option<String>,
        program: Option<AccountId>,
        until: Option<u64>,
    ) -> Result<Self> {
        let list = match (snapshot, root, program) {
            (Some(path), _, _) => {
                let snapshot = Snapshot::load(&path)?;
                if snapshot.leaves().is_empty() {
                    bail!("{} holds no records", path.display());
                }
                Some((snapshot.root(), snapshot.program))
            }
            (None, Some(root), Some(program)) => {
                let bytes = hex::decode(&root).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
                Some((bytes.ok_or_else(|| anyhow!("--claims-root must be 64 hex characters"))?, program))
            }
            _ => None,
        };
        Ok(Self { list, until })
    }

    /// The claim fields, over those in `current`.
    #[must_use]
    pub fn over(self, current: Option<&Settings>) -> Claims {
        let (root, program) = self.list.unwrap_or_else(|| {
            current.map_or(([0; 32], AccountId::new([0; 32])), |settings| (settings.claims_root, settings.claims_program))
        });
        let until = self.until.or_else(|| current.map(|settings| settings.claims_until)).unwrap_or(0);
        Claims { root, until, program }
    }
}

#[cfg(test)]
mod tests {
    use names_core::{MIN_COMMIT_AGE_MS, owner_tag};

    use super::*;

    const PROGRAM: AccountId = AccountId::new([0x38; 32]);
    const ALICE: AccountId = AccountId::new([1; 32]);

    fn snapshot(ids: &[(NameId, AccountId)], extra: Vec<(AccountId, Vec<u8>)>) -> Snapshot {
        let mut accounts: Vec<(AccountId, Vec<u8>)> = ids
            .iter()
            .map(|(id, holder)| {
                let mut data = vec![OLD_VERSION];
                data.extend_from_slice(&owner_tag(id, holder));
                data.extend_from_slice(&u64::MAX.to_le_bytes());
                data.extend_from_slice(&3u32.to_le_bytes());
                data.extend_from_slice(b"old");
                (record_address(&PROGRAM, id), data)
            })
            .collect();
        accounts.extend(extra);
        Snapshot { program: PROGRAM, height: 1, accounts }
    }

    /// The proof must hold against what `names_core` checks on chain, which is private
    /// to it: rebuilt here from the same public pieces.
    #[test]
    fn every_record_gets_a_leaf_and_a_proof_to_the_root() {
        let ids: Vec<(NameId, AccountId)> = (0..7).map(|i| ([i + 1; 32], ALICE)).collect();
        // A commitment and a page, which must not become leaves.
        let page = (page_address(&PROGRAM, &ids[0].0), {
            let mut data = vec![OLD_VERSION];
            data.extend_from_slice(&4u32.to_le_bytes());
            data.extend_from_slice(b"page");
            data
        });
        let commitment = (AccountId::new([9; 32]), vec![OLD_VERSION; 9]);
        let taken = snapshot(&ids, vec![commitment, page]);
        assert_eq!(taken.leaves().len(), ids.len());
        for (id, holder) in &ids {
            let (_, record) = taken.claim(id).expect("in the list");
            assert_eq!(record.owner, owner_tag(id, holder));
            assert_eq!(record.sealed, b"old");
        }
        assert_eq!(taken.page(&ids[0].0).expect("a page"), b"page");
        assert_eq!(taken.page(&ids[1].0), None);
        assert!(taken.claim(&[0xEE; 32]).is_none());
    }

    #[test]
    fn the_list_does_not_depend_on_the_snapshots_order() {
        let ids: Vec<(NameId, AccountId)> = (0..5).map(|i| ([i + 1; 32], ALICE)).collect();
        let mut reversed = ids.clone();
        reversed.reverse();
        assert_eq!(snapshot(&ids, Vec::new()).root(), snapshot(&reversed, Vec::new()).root());
    }

    #[test]
    fn claim_settings_are_kept_unless_changed() {
        let current = Settings {
            version: 6,
            admin: ALICE,
            treasury: ALICE,
            prices: [0; 3],
            mode: 1,
            deposits: [0; 2],
            claims_root: [7; 32],
            claims_until: MIN_COMMIT_AGE_MS,
            claims_program: PROGRAM,
        };
        let kept = ClaimSettings { list: None, until: None }.over(Some(&current));
        assert_eq!((kept.root, kept.until, kept.program), ([7; 32], MIN_COMMIT_AGE_MS, PROGRAM));
        let extended = ClaimSettings { list: None, until: Some(9) }.over(Some(&current));
        assert_eq!((extended.root, extended.until), ([7; 32], 9));
        assert_eq!(ClaimSettings { list: None, until: None }.over(None).root, [0; 32]);
    }
}
