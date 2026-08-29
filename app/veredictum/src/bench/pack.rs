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

/// The `Vital signs` operational template the community harness uploads,
/// embedded byte-identically from the vendored CKM template pack (CKM cid
/// 1013.26.380; template id `Vital signs`, root
/// `openEHR-EHR-COMPOSITION.encounter.v1`).
const VITAL_SIGNS_OPT: &str = include_str!("fixtures/vital_signs.opt");

/// The pinned digest of [`VITAL_SIGNS_OPT`].
const VITAL_SIGNS_OPT_SHA256: &str =
    "3a0d31bd3b5dc6329e53c0d6f22fdbaece62c684136b86139d0729cff8796128";

/// The `Vital signs` `COMPOSITION` instance the community harness commits,
/// byte-identical to the attachment on post 8 of
/// <https://discourse.openehr.org/t/17224>: eight `OBSERVATION` entries under
/// `openEHR-EHR-COMPOSITION.encounter.v1`, `rm_version` 1.0.2.
const VITAL_SIGNS_COMPOSITION: &str = include_str!("fixtures/vital_signs_composition.json");

/// The pinned digest of [`VITAL_SIGNS_COMPOSITION`].
const VITAL_SIGNS_COMPOSITION_SHA256: &str =
    "468081c259c737d35d7f80403562b3f333e479d267286faf80fd7c087eaba947";

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
    /// `GET /ehr/{ehr_id}/composition/{uid}?version_at_time=…` — read the
    /// version of a composition that was current at one instant.
    GetCompositionAtTime,
    /// `GET /ehr/{ehr_id}/composition/{uid}` — read a committed composition
    /// at its latest version.
    GetCompositionLatest,
    /// `GET /ehr/{ehr_id}` — read the EHR resource.
    GetEhr,
    /// `GET /ehr/{ehr_id}/ehr_status` — read the EHR's status resource.
    GetEhrStatus,
    /// `GET /ehr/{ehr_id}/versioned_composition/{uid}` — read the
    /// `VERSIONED_COMPOSITION` container itself.
    GetVersionedComposition,
    /// `GET /ehr/{ehr_id}/versioned_composition/{uid}/revision_history` —
    /// read the versioned object's revision history.
    GetVersionedCompositionRevisionHistory,
    /// `GET /ehr/{ehr_id}/versioned_composition/{uid}/version?version_at_time=…`
    /// — read the version current at one instant.
    GetVersionedCompositionVersionAtTime,
    /// `GET /ehr/{ehr_id}/versioned_composition/{uid}/version/{version_uid}` —
    /// read one version by its own identifier.
    GetVersionedCompositionVersionById,
    /// `GET /ehr/{ehr_id}/versioned_composition/{uid}/version` — read the
    /// latest version of a versioned composition.
    GetVersionedCompositionVersionLatest,
    /// `POST /query/aql` — an EHR-scoped `SELECT c/uid/value` projection.
    AdhocQueryUid,
}

impl BenchOp {
    /// Every operation, in the fixed order every emitted document uses.
    pub const ALL: &[BenchOp] = &[
        BenchOp::AdhocQueryUid,
        BenchOp::CreateComposition,
        BenchOp::GetCompositionAtTime,
        BenchOp::GetCompositionLatest,
        BenchOp::GetEhr,
        BenchOp::GetEhrStatus,
        BenchOp::GetVersionedComposition,
        BenchOp::GetVersionedCompositionRevisionHistory,
        BenchOp::GetVersionedCompositionVersionAtTime,
        BenchOp::GetVersionedCompositionVersionById,
        BenchOp::GetVersionedCompositionVersionLatest,
    ];

    /// The wire token, which is also the key the result records it under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            BenchOp::CreateComposition => "create_composition",
            BenchOp::GetCompositionAtTime => "get_composition_at_time",
            BenchOp::GetCompositionLatest => "get_composition_latest",
            BenchOp::GetEhr => "get_ehr",
            BenchOp::GetEhrStatus => "get_ehr_status",
            BenchOp::GetVersionedComposition => "get_versioned_composition",
            BenchOp::GetVersionedCompositionRevisionHistory => {
                "get_versioned_composition_revision_history"
            }
            BenchOp::GetVersionedCompositionVersionAtTime => {
                "get_versioned_composition_version_at_time"
            }
            BenchOp::GetVersionedCompositionVersionById => {
                "get_versioned_composition_version_by_id"
            }
            BenchOp::GetVersionedCompositionVersionLatest => {
                "get_versioned_composition_version_latest"
            }
            BenchOp::AdhocQueryUid => "adhoc_query_uid",
        }
    }

    /// Whether the operation addresses one seeded composition rather than an
    /// EHR, which decides which draw selects its target.
    #[must_use]
    pub const fn addresses_a_composition(self) -> bool {
        match self {
            BenchOp::CreateComposition
            | BenchOp::GetEhr
            | BenchOp::GetEhrStatus
            | BenchOp::AdhocQueryUid => false,
            BenchOp::GetCompositionAtTime
            | BenchOp::GetCompositionLatest
            | BenchOp::GetVersionedComposition
            | BenchOp::GetVersionedCompositionRevisionHistory
            | BenchOp::GetVersionedCompositionVersionAtTime
            | BenchOp::GetVersionedCompositionVersionById
            | BenchOp::GetVersionedCompositionVersionLatest => true,
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

/// A closed-loop sequential walk over the whole seeded population.
///
/// Where a [`MeasurePhase`] offers arrivals on a schedule and reports
/// coordinated-omission-free percentiles, a sweep issues each request only
/// after the previous one answered, exactly as a single-client harness does,
/// and reports the whole-loop average the closed-loop discipline yields. Both
/// numbers are labelled with the regime that produced them, because they
/// answer different questions and are never interchangeable.
#[derive(Debug, Clone)]
pub struct SweepPhase {
    /// The phase name, as it appears in the result.
    pub name: String,
    /// The operations offered against every seeded composition, in this
    /// order.
    pub per_composition: Vec<BenchOp>,
    /// The closed worker pool the walk runs on. One worker reproduces a
    /// sequential single-client harness.
    pub workers: usize,
}

impl SweepPhase {
    /// How many requests the walk issues over `compositions` compositions.
    #[must_use]
    pub fn requests(&self, compositions: usize) -> usize {
        compositions.saturating_mul(self.per_composition.len())
    }
}

/// One phase of a pack.
#[derive(Debug, Clone)]
pub enum BenchPhase {
    /// A closed-loop bulk load.
    Seed(SeedPhase),
    /// A closed-loop sequential walk over the seeded population.
    Sweep(SweepPhase),
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
                BenchPhase::Sweep(_) | BenchPhase::Measure(_) => None,
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
                BenchPhase::Seed(_) | BenchPhase::Sweep(_) => None,
            })
            .collect()
    }

    /// The closed-loop sweep phases, in execution order.
    #[must_use]
    pub fn sweep_phases(&self) -> Vec<&SweepPhase> {
        self.phases
            .iter()
            .filter_map(|phase| match phase {
                BenchPhase::Sweep(sweep) => Some(sweep),
                BenchPhase::Seed(_) | BenchPhase::Measure(_) => None,
            })
            .collect()
    }
}

/// The id every embedded pack is known by.
const SMOKE: PackId = PackId("smoke");

/// The id of the community-harness reproduction.
const COMMUNITY_VITALS: PackId = PackId("community-vitals");

/// The seven composition reads the community harness issues against every
/// committed composition, in the order it issues them.
const COMMUNITY_READS: &[BenchOp] = &[
    BenchOp::GetCompositionLatest,
    BenchOp::GetCompositionAtTime,
    BenchOp::GetVersionedComposition,
    BenchOp::GetVersionedCompositionVersionLatest,
    BenchOp::GetVersionedCompositionVersionAtTime,
    BenchOp::GetVersionedCompositionVersionById,
    BenchOp::GetVersionedCompositionRevisionHistory,
];

/// The embedded pack ids, in the order `--pack` accepts them.
pub const EMBEDDED: &[PackId] = &[COMMUNITY_VITALS, SMOKE];

/// Loads one embedded pack by its id, verifying every fixture pin.
///
/// # Errors
/// [`BenchError::UnknownPack`] for a token no embedded pack answers to, or
/// [`BenchError::FixturePin`] when an embedded fixture's bytes moved.
pub fn load(token: &str) -> Result<BenchPack, BenchError> {
    let pack = match token {
        "smoke" => smoke(),
        "community-vitals" => community_vitals(),
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

/// EHRs the community harness creates at scale 1.0.
const COMMUNITY_EHRS: usize = 100;

/// Compositions the community harness commits into each EHR.
const COMMUNITY_COMPOSITIONS_PER_EHR: usize = 1_000;

/// The arrival rate the open-loop half of the read phase is pinned at.
const COMMUNITY_READ_RATE_PER_S: f64 = 200.0;

/// The `community-vitals` pack: the openEHR community's own vital-signs
/// harness, reproduced closed-loop and measured again open-loop.
///
/// The write phase reproduces the harness bulk load: 100 EHRs, 1,000
/// commits of the same composition bytes into each, one worker. The read
/// phase runs twice over the population it left behind — once as the
/// sequential walk the harness performs, once as an open-loop arrival
/// schedule at the rate this pack version pins, so the figure that compares
/// with the published one and the figure that survives a stall both appear,
/// each labelled with the regime that produced it.
#[must_use]
pub fn community_vitals() -> BenchPack {
    let fixtures = vec![
        Fixture {
            key: FixtureKey("vital_signs.opt"),
            kind: FixtureKind::OperationalTemplate,
            bytes: VITAL_SIGNS_OPT,
            sha256: VITAL_SIGNS_OPT_SHA256,
        },
        Fixture {
            key: FixtureKey("vital_signs_composition.json"),
            kind: FixtureKind::Composition,
            bytes: VITAL_SIGNS_COMPOSITION,
            sha256: VITAL_SIGNS_COMPOSITION_SHA256,
        },
    ];
    BenchPack {
        id: COMMUNITY_VITALS,
        version: "1.0.0".to_owned(),
        description: COMMUNITY_VITALS_DESCRIPTION.to_owned(),
        seed: 0x436f_6d6d_5f56_6974,
        phases: vec![
            BenchPhase::Seed(SeedPhase {
                name: "write".to_owned(),
                fixtures,
                ehrs: COMMUNITY_EHRS,
                compositions_per_ehr: COMMUNITY_COMPOSITIONS_PER_EHR,
                workers: 1,
            }),
            BenchPhase::Sweep(SweepPhase {
                name: "read_walk".to_owned(),
                per_composition: COMMUNITY_READS.to_vec(),
                workers: 1,
            }),
            BenchPhase::Measure(MeasurePhase {
                name: "read_open_loop".to_owned(),
                rate_per_s: COMMUNITY_READ_RATE_PER_S,
                warmup_s: 15,
                duration_s: 60,
                mix: COMMUNITY_READS.iter().map(|op| (*op, 1)).collect(),
            }),
        ],
    }
}

/// What the `community-vitals` pack exercises, and where its bytes come from.
const COMMUNITY_VITALS_DESCRIPTION: &str = "\
Reproduces the openEHR community's vital-signs benchmark harness \
(<https://discourse.openehr.org/t/17224>) and measures the same work a second \
way. The write phase creates 100 EHRs and commits the same Vital signs \
composition 1,000 times into each with Prefer: return=identifier, on one \
worker, and reports bulk-load throughput plus the whole-loop \
milliseconds-per-composition average the thread quotes, labelled closed-loop. \
The read phase then runs twice: read_walk is the sequential walk over every \
committed composition, seven GETs each (latest, version_at_time, the \
VERSIONED_COMPOSITION, its latest version, its version at that instant, one \
version by id, and the revision history), reporting the whole-loop \
microseconds-per-request average, labelled closed-loop; read_open_loop offers \
the same seven reads as an arrival schedule pinned at 200/s for 60s after a \
15s warmup, which is where the coordinated-omission-free percentiles come \
from. The pinned rate is part of this pack version: changing it changes the \
work and bumps the version. Every version_at_time read addresses one instant \
captured after the write phase finished, which every seeded version predates, \
so it selects the same versions the harness's own start-of-run instant \
selects. Fixture provenance: the operational template is the vendored CKM \
export for template id 'Vital signs' (CKM cid 1013.26.380), byte-identical; \
the composition is the attachment on post 8 of that thread, byte-identical. \
Both are pinned by sha256 and verified at load.";

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
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

    /// The community pack pins both fixtures at the digests the source
    /// material hashes to, so an edited byte fails the load.
    #[test]
    fn the_community_pack_pins_its_source_fixtures() -> Result<(), BenchError> {
        let deck = load("community-vitals")?;
        assert_eq!(deck.id, COMMUNITY_VITALS);
        assert_eq!(deck.version, "1.0.0");
        let pins = deck.fixture_pins();
        assert_eq!(
            pins.get("vital_signs.opt").map(String::as_str),
            Some("3a0d31bd3b5dc6329e53c0d6f22fdbaece62c684136b86139d0729cff8796128")
        );
        assert_eq!(
            pins.get("vital_signs_composition.json").map(String::as_str),
            Some("468081c259c737d35d7f80403562b3f333e479d267286faf80fd7c087eaba947")
        );
        assert_eq!(pins.len(), 2);
        deck.verify_pins()
    }

    /// The embedded composition declares the template the embedded
    /// operational template defines, so the write phase commits something the
    /// upload constrains.
    #[test]
    #[expect(
        clippy::disallowed_types,
        reason = "the approved wire-body seam: the embedded composition is a JSON document read for two attributes"
    )]
    fn the_community_composition_names_the_embedded_template()
    -> Result<(), Box<dyn std::error::Error>> {
        let document: serde_json::Value = serde_json::from_str(VITAL_SIGNS_COMPOSITION)?;
        assert_eq!(
            document
                .pointer("/archetype_details/template_id/value")
                .and_then(serde_json::Value::as_str),
            Some("Vital signs")
        );
        assert_eq!(
            document
                .pointer("/archetype_node_id")
                .and_then(serde_json::Value::as_str),
            Some("openEHR-EHR-COMPOSITION.encounter.v1")
        );
        assert!(
            VITAL_SIGNS_OPT.contains("<template_id><value>Vital signs</value></template_id>")
                || VITAL_SIGNS_OPT.contains("Vital signs"),
            "the embedded template does not carry the template id the composition names"
        );
        Ok(())
    }

    /// The pack carries exactly the three phases the reproduction needs, one
    /// per discipline the record then labels.
    #[test]
    fn the_community_pack_carries_one_phase_per_discipline() {
        let deck = community_vitals();
        assert_eq!(deck.phases.len(), 3);
        let seeds: Vec<&SeedPhase> = deck
            .phases
            .iter()
            .filter_map(|phase| match phase {
                BenchPhase::Seed(seed) => Some(seed),
                BenchPhase::Sweep(_) | BenchPhase::Measure(_) => None,
            })
            .collect();
        assert_eq!(seeds.len(), 1);
        assert_eq!(deck.sweep_phases().len(), 1);
        assert_eq!(deck.measure_phases().len(), 1);
        let Some(write) = seeds.first() else {
            panic!("the write phase is gone");
        };
        assert_eq!(write.ehrs, 100);
        assert_eq!(write.compositions_per_ehr, 1000);
        assert_eq!(write.workers, 1, "the reproduction is sequential");
    }

    /// At scale 1.0 the sweep issues exactly the 700,000 requests the
    /// community harness reports: seven reads over 100,000 compositions.
    #[test]
    fn the_seven_variant_walk_sums_to_the_published_request_count() {
        let deck = community_vitals();
        let Some(sweep) = deck.sweep_phases().first().copied() else {
            panic!("the read walk is gone");
        };
        assert_eq!(sweep.per_composition.len(), 7);
        assert_eq!(sweep.workers, 1);
        let compositions = COMMUNITY_EHRS.saturating_mul(COMMUNITY_COMPOSITIONS_PER_EHR);
        assert_eq!(compositions, 100_000);
        assert_eq!(sweep.requests(compositions), 700_000);
        let distinct: std::collections::BTreeSet<BenchOp> =
            sweep.per_composition.iter().copied().collect();
        assert_eq!(distinct.len(), 7, "a variant is repeated");
        assert!(
            sweep
                .per_composition
                .iter()
                .all(|op| op.addresses_a_composition()),
            "the walk offers an operation that does not address a composition"
        );
    }

    /// The open-loop half offers the same seven reads at equal share, at the
    /// rate the pack version pins.
    #[test]
    fn the_open_loop_half_mirrors_the_walk_at_a_pinned_rate() {
        let deck = community_vitals();
        let Some(measure) = deck.measure_phases().first().copied() else {
            panic!("the open-loop read phase is gone");
        };
        assert!((measure.rate_per_s - 200.0).abs() < f64::EPSILON);
        assert_eq!(measure.warmup_s, 15);
        assert_eq!(measure.duration_s, 60);
        assert_eq!(measure.total_share(), 7);
        let offered: Vec<BenchOp> = measure.mix.iter().map(|(op, _)| *op).collect();
        assert_eq!(offered, COMMUNITY_READS.to_vec());
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
