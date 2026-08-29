// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The embedded benchmark packs: the operation vocabulary, the pinned
//! fixtures, and the phase model.
//!
//! A pack is compiled into the binary, so a bench run needs no artifact root
//! and no catalogue. Every fixture carries a sha256 pin verified when the
//! pack is loaded and recorded in the result, so a reader of two results can
//! see whether the same bytes were offered to both systems.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest as _, Sha256};

use crate::bench::BenchError;

/// The blood-pressure operational template every embedded pack seeds with.
///
/// The template id is `cnf.blood_pressure`, and the compositions the packs
/// commit declare exactly that id in `archetype_details.template_id`
/// (openEHR RM `common` §ARCHETYPED).
const BLOOD_PRESSURE_OPT: &str = include_str!("fixtures/blood_pressure.opt");

/// The pinned digest of [`BLOOD_PRESSURE_OPT`].
const BLOOD_PRESSURE_OPT_SHA256: &str =
    "97549fb2ab7ca36b9baa1cc86e857ef82924927a42140dfd3fd09a05dd83d006";

/// One canonical-JSON `COMPOSITION` constrained by [`BLOOD_PRESSURE_OPT`]:
/// a single `POINT_EVENT` carrying a systolic and a diastolic
/// `DV_QUANTITY` in `mm[Hg]` under the
/// `openEHR-EHR-OBSERVATION.blood_pressure.v2` archetype (openEHR RM
/// `data_structures` §`HISTORY`/`POINT_EVENT`).
const BP_COMPOSITION: &str = include_str!("fixtures/bp_composition.json");

/// The pinned digest of [`BP_COMPOSITION`].
const BP_COMPOSITION_SHA256: &str =
    "9eaea10c5171d1f4648c8e932a21ce624312a2cad98f49115f35efbbb344a3ce";

/// The id of an embedded pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackId(&'static str);

impl PackId {
    /// The id as written.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for PackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The key one embedded fixture is recorded under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixtureKey(&'static str);

impl FixtureKey {
    /// The key as written.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for FixtureKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// What a seeded fixture is, which decides how the seeder offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureKind {
    /// An ADL 1.4 operational template, uploaded once before any EHR exists.
    OperationalTemplate,
    /// A canonical-JSON `COMPOSITION`, committed into every seeded EHR.
    Composition,
}

impl FixtureKind {
    /// The media type the fixture goes on the wire as.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            FixtureKind::OperationalTemplate => "application/xml",
            FixtureKind::Composition => "application/json",
        }
    }
}

/// One embedded fixture with its digest pin.
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    /// The key the pin is recorded under in the result.
    pub key: FixtureKey,
    /// What the fixture is.
    pub kind: FixtureKind,
    /// The embedded bytes.
    pub bytes: &'static str,
    /// The lowercase-hex sha256 the bytes must hash to.
    pub sha256: &'static str,
}

impl Fixture {
    /// Verifies the embedded bytes against the declared pin.
    ///
    /// # Errors
    /// [`BenchError::FixturePin`] naming both digests when they differ.
    pub fn verify(&self, pack: PackId) -> Result<(), BenchError> {
        let actual = hex(&Sha256::digest(self.bytes.as_bytes()));
        if actual == self.sha256 {
            return Ok(());
        }
        Err(BenchError::FixturePin {
            pack: pack.as_str().to_owned(),
            fixture: self.key.as_str().to_owned(),
            expected: self.sha256.to_owned(),
            actual,
        })
    }
}

/// Lowercase hex, the encoding every pin in a pack carries.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len().saturating_mul(2)),
        |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        },
    )
}

/// The closed operation vocabulary a measured phase draws its arrivals from.
///
/// Every variant is one ITS-REST exchange. An unknown token is a loud error,
/// never a fallback: a typo that silently became a default would manufacture
/// a passing row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenchOp {
    /// `POST /ehr/{ehr_id}/composition` — commit a new composition.
    CreateComposition,
    /// `GET /ehr/{ehr_id}/composition/{uid}` — read a committed composition
    /// at its latest version.
    GetCompositionLatest,
    /// `GET /ehr/{ehr_id}` — read the EHR resource.
    GetEhr,
    /// `GET /ehr/{ehr_id}/ehr_status` — read the EHR's status resource.
    GetEhrStatus,
    /// `POST /query/aql` — an EHR-scoped `SELECT c/uid/value` projection.
    AdhocQueryUid,
}

impl BenchOp {
    /// Every operation, in the fixed order every emitted document uses.
    pub const ALL: &[BenchOp] = &[
        BenchOp::AdhocQueryUid,
        BenchOp::CreateComposition,
        BenchOp::GetCompositionLatest,
        BenchOp::GetEhr,
        BenchOp::GetEhrStatus,
    ];

    /// The wire token, which is also the key the result records it under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            BenchOp::CreateComposition => "create_composition",
            BenchOp::GetCompositionLatest => "get_composition_latest",
            BenchOp::GetEhr => "get_ehr",
            BenchOp::GetEhrStatus => "get_ehr_status",
            BenchOp::AdhocQueryUid => "adhoc_query_uid",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`BenchError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, BenchError> {
        Self::ALL
            .iter()
            .copied()
            .find(|op| op.as_str() == token)
            .ok_or_else(|| BenchError::UnknownToken {
                vocabulary: "bench operation",
                token: token.to_owned(),
                accepted: accepted_ops(),
            })
    }
}

impl fmt::Display for BenchOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The accepted operation tokens, comma-separated, for a rejection message.
fn accepted_ops() -> String {
    BenchOp::ALL
        .iter()
        .map(|op| op.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A closed-loop bulk load: the corpus every measured phase then reads and
/// writes against.
#[derive(Debug, Clone)]
pub struct SeedPhase {
    /// The phase name, as it appears in the result.
    pub name: String,
    /// The fixtures this phase offers, in offer order.
    pub fixtures: Vec<Fixture>,
    /// How many EHRs to create.
    pub ehrs: usize,
    /// How many compositions to commit into each EHR.
    pub compositions_per_ehr: usize,
    /// The closed worker pool the bulk load runs on.
    pub workers: usize,
}

/// An open-loop measured phase: a seeded arrival schedule at a pinned rate
/// over an operation mix.
#[derive(Debug, Clone)]
pub struct MeasurePhase {
    /// The phase name, as it appears in the result.
    pub name: String,
    /// The aggregate arrival rate (arrivals per second).
    pub rate_per_s: f64,
    /// Warmup seconds, dispatched but excluded from the measured statistics.
    pub warmup_s: u64,
    /// The measured span in seconds.
    pub duration_s: u64,
    /// The operation mix as (operation, share) pairs. Shares are relative;
    /// the engine normalizes over their sum.
    pub mix: Vec<(BenchOp, u32)>,
}

impl MeasurePhase {
    /// The sum of every share in the mix.
    #[must_use]
    pub fn total_share(&self) -> u64 {
        self.mix
            .iter()
            .map(|(_, share)| u64::from(*share))
            .fold(0_u64, u64::saturating_add)
    }

    /// The operation a seeded draw selects, or `None` when the mix is empty
    /// or every share is zero.
    #[must_use]
    pub fn op_for_draw(&self, draw: u64) -> Option<BenchOp> {
        let total = self.total_share();
        if total == 0 {
            return None;
        }
        let mut point = draw % total;
        for (op, share) in &self.mix {
            let share = u64::from(*share);
            if point < share {
                return Some(*op);
            }
            point = point.saturating_sub(share);
        }
        self.mix.last().map(|(op, _)| *op)
    }
}

/// One phase of a pack.
#[derive(Debug, Clone)]
pub enum BenchPhase {
    /// A closed-loop bulk load.
    Seed(SeedPhase),
    /// An open-loop measured phase.
    Measure(MeasurePhase),
}

/// A versioned, embedded benchmark pack.
#[derive(Debug, Clone)]
pub struct BenchPack {
    /// The pack id.
    pub id: PackId,
    /// The pack version, bumped whenever a phase or a fixture changes, so
    /// two results are only comparable when it matches.
    pub version: String,
    /// What the pack exercises, in one sentence.
    pub description: String,
    /// The seed the arrival streams draw from, disclosed in the result.
    pub seed: u64,
    /// The phases, in execution order.
    pub phases: Vec<BenchPhase>,
}

impl BenchPack {
    /// Replaces the phases, keeping the pack's identity and seed.
    #[must_use]
    pub fn with_phases(mut self, phases: Vec<BenchPhase>) -> Self {
        self.phases = phases;
        self
    }

    /// Every fixture the pack's seed phases offer, in phase order.
    #[must_use]
    pub fn fixtures(&self) -> Vec<Fixture> {
        self.phases
            .iter()
            .filter_map(|phase| match phase {
                BenchPhase::Seed(seed) => Some(seed.fixtures.clone()),
                BenchPhase::Measure(_) => None,
            })
            .flatten()
            .collect()
    }

    /// The fixture pins, keyed by fixture key, as the result records them.
    #[must_use]
    pub fn fixture_pins(&self) -> BTreeMap<String, String> {
        self.fixtures()
            .into_iter()
            .map(|fixture| (fixture.key.as_str().to_owned(), fixture.sha256.to_owned()))
            .collect()
    }

    /// Verifies every embedded fixture against its pin.
    ///
    /// # Errors
    /// [`BenchError::FixturePin`] for the first fixture whose bytes moved.
    pub fn verify_pins(&self) -> Result<(), BenchError> {
        for fixture in self.fixtures() {
            fixture.verify(self.id)?;
        }
        Ok(())
    }

    /// The measured phases, in execution order.
    #[must_use]
    pub fn measure_phases(&self) -> Vec<&MeasurePhase> {
        self.phases
            .iter()
            .filter_map(|phase| match phase {
                BenchPhase::Measure(measure) => Some(measure),
                BenchPhase::Seed(_) => None,
            })
            .collect()
    }
}

/// The id every embedded pack is known by.
const SMOKE: PackId = PackId("smoke");

/// The embedded pack ids, in the order `--pack` accepts them.
pub const EMBEDDED: &[PackId] = &[SMOKE];

/// Loads one embedded pack by its id, verifying every fixture pin.
///
/// # Errors
/// [`BenchError::UnknownPack`] for a token no embedded pack answers to, or
/// [`BenchError::FixturePin`] when an embedded fixture's bytes moved.
pub fn load(token: &str) -> Result<BenchPack, BenchError> {
    let pack = match token {
        "smoke" => smoke(),
        other => {
            return Err(BenchError::UnknownPack {
                requested: other.to_owned(),
                known: EMBEDDED
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        }
    };
    pack.verify_pins()?;
    Ok(pack)
}

/// The `smoke` pack: one small bulk load, then one short open-loop phase
/// over the whole operation vocabulary.
#[must_use]
pub fn smoke() -> BenchPack {
    BenchPack {
        id: SMOKE,
        version: "1.0.0".to_owned(),
        description: "One blood-pressure template, a small EHR corpus, and a mixed open-loop phase over the read, write and query surface.".to_owned(),
        seed: 0x5645_5245_4449_4354,
        phases: vec![
            BenchPhase::Seed(SeedPhase {
                name: "seed".to_owned(),
                fixtures: vec![
                    Fixture {
                        key: FixtureKey("blood_pressure.opt"),
                        kind: FixtureKind::OperationalTemplate,
                        bytes: BLOOD_PRESSURE_OPT,
                        sha256: BLOOD_PRESSURE_OPT_SHA256,
                    },
                    Fixture {
                        key: FixtureKey("bp_composition.json"),
                        kind: FixtureKind::Composition,
                        bytes: BP_COMPOSITION,
                        sha256: BP_COMPOSITION_SHA256,
                    },
                ],
                ehrs: 200,
                compositions_per_ehr: 5,
                workers: 8,
            }),
            BenchPhase::Measure(MeasurePhase {
                name: "mixed".to_owned(),
                rate_per_s: 50.0,
                warmup_s: 10,
                duration_s: 60,
                mix: vec![
                    (BenchOp::CreateComposition, 20),
                    (BenchOp::GetCompositionLatest, 30),
                    (BenchOp::GetEhr, 20),
                    (BenchOp::GetEhrStatus, 15),
                    (BenchOp::AdhocQueryUid, 15),
                ],
            }),
        ],
    }
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "two Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// Every embedded pack loads with its pins intact. A fixture edited
    /// without its pin moved fails here, which is the whole point of the pin.
    #[test]
    fn every_embedded_pack_loads_with_verified_pins() -> Result<(), BenchError> {
        for id in EMBEDDED {
            let pack = load(id.as_str())?;
            assert_eq!(pack.id, *id);
            assert!(!pack.fixtures().is_empty(), "{id} embeds no fixture");
            pack.verify_pins()?;
        }
        Ok(())
    }

    /// A moved pin is refused by key, never silently accepted.
    #[test]
    fn a_moved_pin_is_refused() {
        let fixture = Fixture {
            key: FixtureKey("moved"),
            kind: FixtureKind::Composition,
            bytes: "{}",
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        let error = fixture.verify(SMOKE).unwrap_err();
        assert!(matches!(error, BenchError::FixturePin { .. }), "{error}");
        assert!(error.to_string().contains("moved"), "{error}");
    }

    /// An unknown pack token is a loud error listing what is embedded.
    #[test]
    fn an_unknown_pack_is_refused() {
        let error = load("does-not-exist").unwrap_err();
        assert!(error.to_string().contains("smoke"), "{error}");
    }

    /// An unknown operation token never falls back to a default.
    #[test]
    fn an_unknown_operation_token_is_refused() {
        assert!(BenchOp::parse("create_composition").is_ok());
        let error = BenchOp::parse("create_compositon").unwrap_err();
        assert!(matches!(error, BenchError::UnknownToken { .. }), "{error}");
    }

    /// Every operation's token round-trips, so the result's keys and the
    /// vocabulary cannot drift apart.
    #[test]
    fn every_operation_token_round_trips() -> Result<(), BenchError> {
        for op in BenchOp::ALL {
            assert_eq!(BenchOp::parse(op.as_str())?, *op);
        }
        Ok(())
    }

    /// `ALL` is sorted by token, which is the order the emitted schema and
    /// every rendered table read.
    #[test]
    fn the_operation_vocabulary_is_token_sorted() {
        let mut sorted: Vec<&str> = BenchOp::ALL.iter().map(|op| op.as_str()).collect();
        let listed = sorted.clone();
        sorted.sort_unstable();
        assert_eq!(listed, sorted);
    }

    /// The mix picker covers exactly the declared shares, with no operation
    /// outside the mix and no share starved.
    #[test]
    fn the_mix_picker_follows_the_declared_shares() {
        let phase = MeasurePhase {
            name: "t".to_owned(),
            rate_per_s: 1.0,
            warmup_s: 0,
            duration_s: 1,
            mix: vec![(BenchOp::GetEhr, 3), (BenchOp::CreateComposition, 1)],
        };
        let picked: Vec<BenchOp> = (0..8).filter_map(|d| phase.op_for_draw(d)).collect();
        assert_eq!(
            picked,
            vec![
                BenchOp::GetEhr,
                BenchOp::GetEhr,
                BenchOp::GetEhr,
                BenchOp::CreateComposition,
                BenchOp::GetEhr,
                BenchOp::GetEhr,
                BenchOp::GetEhr,
                BenchOp::CreateComposition,
            ]
        );
    }

    /// An empty mix selects nothing rather than defaulting to an operation.
    #[test]
    fn an_empty_mix_selects_nothing() {
        let phase = MeasurePhase {
            name: "t".to_owned(),
            rate_per_s: 1.0,
            warmup_s: 0,
            duration_s: 1,
            mix: Vec::new(),
        };
        assert_eq!(phase.op_for_draw(7), None);
    }
}
