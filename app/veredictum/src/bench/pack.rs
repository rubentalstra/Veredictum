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
use crate::bench::posture::{CLINICAL_DEFAULT, MINIMAL, PostureProfile};

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

/// The invalid twin of [`BP_COMPOSITION`]: the same bytes with the mandatory
/// `COMPOSITION.composer` member deleted and nothing else changed.
///
/// `composer` is `1..1` (openEHR RM `UML/classes/composition.adoc`
/// §Attributes), so a server validating a commit against the reference model
/// and the operational template refuses it (ITS-REST
/// `specifications/responses/422.yaml`, and `422` on
/// `specifications/operations/composition_create.yaml`).
const BP_COMPOSITION_TWIN: &str = include_str!("fixtures/bp_composition.missing_composer.json");

/// The pinned digest of [`BP_COMPOSITION_TWIN`].
const BP_COMPOSITION_TWIN_SHA256: &str =
    "602039bed3f3daf060152af6034baf6d7ce74fde6ec77e8ff1cc89eda2b3e0b3";

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

/// The invalid twin of [`VITAL_SIGNS_COMPOSITION`], derived the same way
/// [`BP_COMPOSITION_TWIN`] is: the mandatory `COMPOSITION.composer` member
/// deleted and nothing else changed.
const VITAL_SIGNS_COMPOSITION_TWIN: &str =
    include_str!("fixtures/vital_signs_composition.missing_composer.json");

/// The pinned digest of [`VITAL_SIGNS_COMPOSITION_TWIN`].
const VITAL_SIGNS_COMPOSITION_TWIN_SHA256: &str =
    "f0598db5ab447b371ead28cba0f841f72370dbbf93db98d5b8e477910a42688d";

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
    /// The invalid twin of that composition, which no phase commits and the
    /// commit-validation canary offers once per bracket.
    InvalidComposition,
}

impl FixtureKind {
    /// The media type the fixture goes on the wire as.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            FixtureKind::OperationalTemplate => "application/xml",
            FixtureKind::Composition | FixtureKind::InvalidComposition => "application/json",
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
    /// `POST /query/aql` — `COUNT` over the population a magnitude threshold
    /// matches (openEHR QUERY `AQL` §Aggregate functions).
    AdhocQueryAggregate,
    /// `POST /query/aql` — every composition in one EHR, projected by uid.
    AdhocQueryEhrScan,
    /// `POST /query/aql` — an EHR-scoped magnitude predicate over the
    /// blood-pressure observation leaves.
    AdhocQueryFiltered,
    /// `POST /query/aql` — an `ORDER BY` over composition start time, read
    /// through a moving fetch window.
    AdhocQueryOrderedPage,
    /// `POST /query/aql` — one composition addressed by its own uid inside
    /// one EHR.
    AdhocQueryPointLookup,
    /// `POST /query/aql` — the magnitude predicate with no EHR scope, bounded
    /// by a fetch count.
    AdhocQueryPopulation,
}

impl BenchOp {
    /// Every operation, in the fixed order every emitted document uses.
    pub const ALL: &[BenchOp] = &[
        BenchOp::AdhocQueryAggregate,
        BenchOp::AdhocQueryEhrScan,
        BenchOp::AdhocQueryFiltered,
        BenchOp::AdhocQueryOrderedPage,
        BenchOp::AdhocQueryPointLookup,
        BenchOp::AdhocQueryPopulation,
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
            BenchOp::AdhocQueryAggregate => "adhoc_query_aggregate",
            BenchOp::AdhocQueryEhrScan => "adhoc_query_ehr_scan",
            BenchOp::AdhocQueryFiltered => "adhoc_query_filtered",
            BenchOp::AdhocQueryOrderedPage => "adhoc_query_ordered_page",
            BenchOp::AdhocQueryPointLookup => "adhoc_query_point_lookup",
            BenchOp::AdhocQueryPopulation => "adhoc_query_population",
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
            | BenchOp::AdhocQueryUid
            | BenchOp::AdhocQueryAggregate
            | BenchOp::AdhocQueryEhrScan
            | BenchOp::AdhocQueryFiltered
            | BenchOp::AdhocQueryOrderedPage
            | BenchOp::AdhocQueryPopulation => false,
            BenchOp::GetCompositionAtTime
            | BenchOp::GetCompositionLatest
            | BenchOp::GetVersionedComposition
            | BenchOp::GetVersionedCompositionRevisionHistory
            | BenchOp::GetVersionedCompositionVersionAtTime
            | BenchOp::GetVersionedCompositionVersionById
            | BenchOp::GetVersionedCompositionVersionLatest
            | BenchOp::AdhocQueryPointLookup => true,
        }
    }

    /// Whether the operation is realized as an ad-hoc AQL query, which is
    /// what decides whether it carries a `POST /query/aql` body.
    #[must_use]
    pub const fn is_adhoc_query(self) -> bool {
        match self {
            BenchOp::AdhocQueryAggregate
            | BenchOp::AdhocQueryEhrScan
            | BenchOp::AdhocQueryFiltered
            | BenchOp::AdhocQueryOrderedPage
            | BenchOp::AdhocQueryPointLookup
            | BenchOp::AdhocQueryPopulation
            | BenchOp::AdhocQueryUid => true,
            BenchOp::CreateComposition
            | BenchOp::GetCompositionAtTime
            | BenchOp::GetCompositionLatest
            | BenchOp::GetEhr
            | BenchOp::GetEhrStatus
            | BenchOp::GetVersionedComposition
            | BenchOp::GetVersionedCompositionRevisionHistory
            | BenchOp::GetVersionedCompositionVersionAtTime
            | BenchOp::GetVersionedCompositionVersionById
            | BenchOp::GetVersionedCompositionVersionLatest => false,
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

/// One entry of a measured phase's operation mix: the operation, its share of
/// the arrivals, and what offering it probes.
///
/// The rationale lives on the entry rather than on [`BenchOp`], because why a
/// pack offers an operation is a property of that pack's design: the same read
/// probes one thing inside a harness reproduction and another inside a query
/// mix. It is part of the versioned pack definition, so changing it changes
/// the pack version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixEntry {
    /// The operation this entry offers.
    pub op: BenchOp,
    /// This entry's share of the arrivals, relative to every other entry's.
    pub share: u32,
    /// What offering this operation probes, in one sentence, for a rendered
    /// legend.
    pub rationale: String,
}

impl MixEntry {
    /// Builds one mix entry.
    #[must_use]
    pub fn new(op: BenchOp, share: u32, rationale: &str) -> Self {
        Self {
            op,
            share,
            rationale: rationale.to_owned(),
        }
    }
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
    /// The operation mix. Shares are relative; the engine normalizes over
    /// their sum.
    pub mix: Vec<MixEntry>,
}

impl MeasurePhase {
    /// The sum of every share in the mix.
    #[must_use]
    pub fn total_share(&self) -> u64 {
        self.mix
            .iter()
            .map(|entry| u64::from(entry.share))
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
        for entry in &self.mix {
            let share = u64::from(entry.share);
            if point < share {
                return Some(entry.op);
            }
            point = point.saturating_sub(share);
        }
        self.mix.last().map(|entry| entry.op)
    }

    /// What each offered operation probes, keyed by the operation token.
    ///
    /// This is the phase's half of the legend a rendered view prints beside
    /// the per-operation numbers.
    #[must_use]
    pub fn rationales(&self) -> BTreeMap<String, String> {
        self.mix
            .iter()
            .map(|entry| (entry.op.as_str().to_owned(), entry.rationale.clone()))
            .collect()
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
    /// The posture profiles this pack version defines, in declaration order.
    ///
    /// A run declares exactly one, and the first is what `--posture` defaults
    /// to. Two results are the same sport only when the same profile stands
    /// behind both, so the set is part of the versioned pack definition.
    pub profiles: Vec<&'static PostureProfile>,
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

    /// The profile a run declares: the named one, or the pack's first.
    ///
    /// # Errors
    /// [`BenchError::UnknownProfile`] for a token this pack does not define,
    /// listing what it does, and [`BenchError::NoProfiles`] for a pack that
    /// defines none.
    pub fn resolve_profile(
        &self,
        token: Option<&str>,
    ) -> Result<&'static PostureProfile, BenchError> {
        let Some(token) = token else {
            return self
                .profiles
                .first()
                .copied()
                .ok_or_else(|| BenchError::NoProfiles {
                    pack: self.id.as_str().to_owned(),
                });
        };
        self.profiles
            .iter()
            .copied()
            .find(|profile| profile.name == token)
            .ok_or_else(|| BenchError::UnknownProfile {
                pack: self.id.as_str().to_owned(),
                requested: token.to_owned(),
                known: self.profile_names().join(", "),
            })
    }

    /// The names of every profile this pack defines, in declaration order.
    #[must_use]
    pub fn profile_names(&self) -> Vec<&'static str> {
        self.profiles.iter().map(|profile| profile.name).collect()
    }

    /// The invalid twin the commit-validation canary offers, when the pack
    /// embeds one.
    #[must_use]
    pub fn invalid_twin(&self) -> Option<Fixture> {
        self.fixtures()
            .into_iter()
            .find(|fixture| fixture.kind == FixtureKind::InvalidComposition)
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

    /// What every measured operation in this pack probes, keyed by the
    /// operation token.
    ///
    /// One key per operation the pack's measured phases offer, which is what a
    /// rendered legend reads to explain a per-operation column without knowing
    /// anything about the pack's internals.
    #[must_use]
    pub fn probe_rationales(&self) -> BTreeMap<String, String> {
        self.measure_phases()
            .into_iter()
            .flat_map(MeasurePhase::rationales)
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

/// The id of the AQL query mix.
const AQL_MIX: PackId = PackId("aql-mix");

/// The seven composition reads the community harness issues against every
/// committed composition, in the order it issues them, each with what
/// offering it probes.
const COMMUNITY_READS: &[(BenchOp, &str)] = &[
    (
        BenchOp::GetCompositionLatest,
        "the harness's latest-version composition read, the read a client issues most",
    ),
    (
        BenchOp::GetCompositionAtTime,
        "the harness's composition read at an instant, which resolves a version by time",
    ),
    (
        BenchOp::GetVersionedComposition,
        "the harness's read of the VERSIONED_COMPOSITION container itself",
    ),
    (
        BenchOp::GetVersionedCompositionVersionLatest,
        "the harness's latest-version read through the versioned object",
    ),
    (
        BenchOp::GetVersionedCompositionVersionAtTime,
        "the harness's version-at-an-instant read through the versioned object",
    ),
    (
        BenchOp::GetVersionedCompositionVersionById,
        "the harness's read of one version by its own identifier",
    ),
    (
        BenchOp::GetVersionedCompositionRevisionHistory,
        "the harness's revision-history read, which walks every version of the object",
    ),
];

/// The embedded pack ids, in the order `--pack` accepts them.
pub const EMBEDDED: &[PackId] = &[AQL_MIX, COMMUNITY_VITALS, SMOKE];

/// Loads one embedded pack by its id, verifying every fixture pin.
///
/// # Errors
/// [`BenchError::UnknownPack`] for a token no embedded pack answers to, or
/// [`BenchError::FixturePin`] when an embedded fixture's bytes moved.
pub fn load(token: &str) -> Result<BenchPack, BenchError> {
    let pack = match token {
        "smoke" => smoke(),
        "community-vitals" => community_vitals(),
        "aql-mix" => aql_mix(),
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
        profiles: vec![&MINIMAL],
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
                    Fixture {
                        key: FixtureKey("bp_composition.missing_composer.json"),
                        kind: FixtureKind::InvalidComposition,
                        bytes: BP_COMPOSITION_TWIN,
                        sha256: BP_COMPOSITION_TWIN_SHA256,
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
                    MixEntry::new(
                        BenchOp::CreateComposition,
                        20,
                        "the commit path, measured while reads compete with it",
                    ),
                    MixEntry::new(
                        BenchOp::GetCompositionLatest,
                        30,
                        "the latest-version composition read",
                    ),
                    MixEntry::new(
                        BenchOp::GetEhr,
                        20,
                        "the EHR resource read, the cheapest addressed read the API offers",
                    ),
                    MixEntry::new(
                        BenchOp::GetEhrStatus,
                        15,
                        "the status read, which reaches a second versioned object in the same EHR",
                    ),
                    MixEntry::new(
                        BenchOp::AdhocQueryUid,
                        15,
                        "an EHR-scoped projection, so the query path is exercised beside the direct reads",
                    ),
                ],
            }),
        ],
    }
}

/// The three pinned fixtures every pack that seeds the Vital signs population
/// offers, in offer order: the template first, then the composition it
/// constrains, then that composition's invalid twin.
fn vital_signs_fixtures() -> Vec<Fixture> {
    vec![
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
        Fixture {
            key: FixtureKey("vital_signs_composition.missing_composer.json"),
            kind: FixtureKind::InvalidComposition,
            bytes: VITAL_SIGNS_COMPOSITION_TWIN,
            sha256: VITAL_SIGNS_COMPOSITION_TWIN_SHA256,
        },
    ]
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
    let fixtures = vital_signs_fixtures();
    BenchPack {
        id: COMMUNITY_VITALS,
        version: "1.0.0".to_owned(),
        description: COMMUNITY_VITALS_DESCRIPTION.to_owned(),
        seed: 0x436f_6d6d_5f56_6974,
        profiles: vec![&MINIMAL, &CLINICAL_DEFAULT],
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
                per_composition: COMMUNITY_READS.iter().map(|(op, _)| *op).collect(),
                workers: 1,
            }),
            BenchPhase::Measure(MeasurePhase {
                name: "read_open_loop".to_owned(),
                rate_per_s: COMMUNITY_READ_RATE_PER_S,
                warmup_s: 15,
                duration_s: 60,
                mix: COMMUNITY_READS
                    .iter()
                    .map(|(op, rationale)| MixEntry::new(*op, 1, rationale))
                    .collect(),
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

/// EHRs the AQL pack seeds.
const AQL_MIX_EHRS: usize = 50;

/// Compositions the AQL pack commits into each seeded EHR.
const AQL_MIX_COMPOSITIONS_PER_EHR: usize = 20;

/// The closed worker pool the AQL pack's bulk load runs on. The load builds a
/// population rather than reproducing a harness, so it uses a pool.
const AQL_MIX_SEED_WORKERS: usize = 8;

/// The aggregate arrival rate the measured query phase is pinned at. Six
/// classes at equal share, so each is offered at four arrivals a second.
const AQL_MIX_RATE_PER_S: f64 = 24.0;

/// Warmup seconds the measured query phase dispatches and then discards.
const AQL_MIX_WARMUP_S: u64 = 15;

/// The measured span of the query phase, in seconds.
const AQL_MIX_DURATION_S: u64 = 60;

/// The six query classes the pack measures, each with the storage behaviour it
/// probes. This table IS the pack's class definition: the mix, the legend and
/// the pack description all read it, so no second copy can drift.
const AQL_MIX_CLASSES: &[(BenchOp, &str)] = &[
    (
        BenchOp::AdhocQueryPointLookup,
        "the indexed-read floor: one composition addressed by its own uid inside one EHR, the cheapest query a server can answer",
    ),
    (
        BenchOp::AdhocQueryEhrScan,
        "the loaded-database shape: every composition in one EHR projected by uid, so the cost follows how much that EHR holds",
    ),
    (
        BenchOp::AdhocQueryFiltered,
        "the value index: a systolic magnitude threshold over the observation leaves of one EHR, with the threshold drawn per arrival so no result set can be memoized",
    ),
    (
        BenchOp::AdhocQueryPopulation,
        "the cross-EHR planner: the same magnitude threshold with no EHR scope and a fetch bound, so the server picks an access path over the whole population",
    ),
    (
        BenchOp::AdhocQueryAggregate,
        "the columnar shape: one COUNT over the population that threshold matches, which returns a single row and reads every value behind it",
    ),
    (
        BenchOp::AdhocQueryOrderedPage,
        "sorting and pagination: an ORDER BY over composition start time read through a moving fetch window, the shape a paged user interface issues",
    ),
];

/// The `aql-mix` pack: query speed over the vital-signs population, one
/// measured class per storage behaviour.
///
/// The seed phase builds the same corpus the community pack builds, from the
/// same two pinned fixtures, at a population sized for query shapes rather
/// than for scale. The measured phase then offers the six query classes
/// open-loop at equal share, so the record carries one set of percentiles per
/// class instead of one blended query number.
#[must_use]
pub fn aql_mix() -> BenchPack {
    BenchPack {
        id: AQL_MIX,
        version: "1.0.0".to_owned(),
        description: aql_mix_description(),
        seed: 0x4151_4c5f_4d69_7800,
        profiles: vec![&MINIMAL],
        phases: vec![
            BenchPhase::Seed(SeedPhase {
                name: "seed".to_owned(),
                fixtures: vital_signs_fixtures(),
                ehrs: AQL_MIX_EHRS,
                compositions_per_ehr: AQL_MIX_COMPOSITIONS_PER_EHR,
                workers: AQL_MIX_SEED_WORKERS,
            }),
            BenchPhase::Measure(MeasurePhase {
                name: "queries".to_owned(),
                rate_per_s: AQL_MIX_RATE_PER_S,
                warmup_s: AQL_MIX_WARMUP_S,
                duration_s: AQL_MIX_DURATION_S,
                mix: AQL_MIX_CLASSES
                    .iter()
                    .map(|(op, rationale)| MixEntry::new(*op, 1, rationale))
                    .collect(),
            }),
        ],
    }
}

/// What the `aql-mix` pack exercises, with its six classes named from the one
/// table that defines them.
fn aql_mix_description() -> String {
    let classes = AQL_MIX_CLASSES
        .iter()
        .map(|(op, rationale)| format!("{op} probes {rationale}"))
        .collect::<Vec<_>>()
        .join("; ");
    format!("{AQL_MIX_PREAMBLE} The six classes: {classes}. {AQL_MIX_PROVENANCE}")
}

/// What the `aql-mix` pack measures and how, ahead of its class list.
const AQL_MIX_PREAMBLE: &str = "\
Measures AQL query speed over the same Vital signs population the \
community-vitals pack seeds, so a query figure and a read figure describe the \
same corpus. The seed phase creates 50 EHRs and commits the same composition \
20 times into each, on a pool of 8 workers. This pack version pins that \
population, and it is sized for query shapes: large enough that a query has to \
choose an access path, small enough to load before a measured window opens. \
The measured phase is open-loop at 24 arrivals a \
second for 60s after a 15s warmup, over six query classes at equal share, so \
each class is offered at 4 arrivals a second and every class returns the same \
number of samples. Each class posts one AQL statement to /query/aql, accepts \
only 200, and counts every other answer in its own error class, so a server \
that refuses one shape never contaminates another class's percentiles. Every \
query parameter draws from the run's seeded streams: the systolic threshold, \
the page offset, and the EHR or composition each arrival addresses, so no \
arrival repeats the previous one's result set and the whole draw is \
reproducible from the seed the record discloses.";

/// Where the `aql-mix` pack's two fixtures come from.
const AQL_MIX_PROVENANCE: &str = "\
Fixture provenance: the operational template is the vendored CKM export for \
template id 'Vital signs' (CKM cid 1013.26.380) and the composition is the \
attachment on post 8 of <https://discourse.openehr.org/t/17224>, both \
byte-identical and pinned by sha256.";

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
            mix: vec![
                MixEntry::new(BenchOp::GetEhr, 3, "the EHR read"),
                MixEntry::new(BenchOp::CreateComposition, 1, "the commit"),
            ],
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
        assert_eq!(
            pins.get("vital_signs_composition.missing_composer.json")
                .map(String::as_str),
            Some("f0598db5ab447b371ead28cba0f841f72370dbbf93db98d5b8e477910a42688d")
        );
        assert_eq!(pins.len(), 3);
        deck.verify_pins()
    }

    /// Every pack defines at least one posture profile, names `minimal` first,
    /// and refuses a profile token it does not define.
    #[test]
    fn every_pack_declares_its_posture_profiles() -> Result<(), BenchError> {
        for id in EMBEDDED {
            let deck = load(id.as_str())?;
            assert!(!deck.profiles.is_empty(), "{id} defines no posture profile");
            assert_eq!(
                deck.resolve_profile(None)?.name,
                "minimal",
                "{id} does not default to the bare spec-conformant surface"
            );
            assert_eq!(deck.resolve_profile(Some("minimal"))?.name, "minimal");
            let error = deck.resolve_profile(Some("hardened")).unwrap_err();
            assert!(
                matches!(error, BenchError::UnknownProfile { .. }),
                "{error}"
            );
            assert!(error.to_string().contains("minimal"), "{error}");
        }
        assert_eq!(
            community_vitals().profile_names(),
            vec!["minimal", "clinical-default"]
        );
        Ok(())
    }

    /// Every pack that seeds a composition also embeds its invalid twin, and
    /// the twin is that composition with the mandatory
    /// `COMPOSITION.composer` [1..1] member gone and nothing else changed
    /// (openEHR RM `UML/classes/composition.adoc` §Attributes).
    #[test]
    #[expect(
        clippy::disallowed_types,
        reason = "the approved wire-body seam: both fixtures are JSON documents compared member by member"
    )]
    fn every_seeded_composition_carries_its_invalid_twin() -> Result<(), Box<dyn std::error::Error>>
    {
        for id in EMBEDDED {
            let deck = load(id.as_str())?;
            let fixtures = deck.fixtures();
            let Some(valid) = fixtures
                .iter()
                .find(|fixture| fixture.kind == FixtureKind::Composition)
            else {
                panic!("{id} seeds no composition");
            };
            let Some(twin) = deck.invalid_twin() else {
                panic!("{id} embeds no invalid twin");
            };
            let mut parent: serde_json::Value = serde_json::from_str(valid.bytes)?;
            let twin_document: serde_json::Value = serde_json::from_str(twin.bytes)?;
            let removed = parent
                .as_object_mut()
                .and_then(|root| root.remove("composer"));
            assert!(removed.is_some(), "{id}: the valid twin has no composer");
            assert_eq!(
                parent, twin_document,
                "{id}: the twin differs by more than the composer"
            );
            assert_eq!(twin.kind.media_type(), "application/json");
            twin.verify(deck.id)?;
        }
        Ok(())
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
        let offered: Vec<BenchOp> = measure.mix.iter().map(|entry| entry.op).collect();
        let declared: Vec<BenchOp> = COMMUNITY_READS.iter().map(|(op, _)| *op).collect();
        assert_eq!(offered, declared);
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

    /// Every mix entry of every embedded pack states what it probes, because
    /// a legend with a blank cell explains nothing.
    #[test]
    fn every_embedded_mix_entry_states_what_it_probes() -> Result<(), BenchError> {
        for id in EMBEDDED {
            let deck = load(id.as_str())?;
            let mut entries = 0_usize;
            for phase in deck.measure_phases() {
                for entry in &phase.mix {
                    assert!(
                        !entry.rationale.trim().is_empty(),
                        "{id}: {} carries no rationale",
                        entry.op
                    );
                    entries = entries.saturating_add(1);
                }
            }
            assert_eq!(
                deck.probe_rationales().len(),
                entries,
                "{id}: the legend lost an entry"
            );
        }
        Ok(())
    }

    /// The AQL pack seeds the same two pinned fixtures the community pack
    /// seeds, so a query figure and a read figure describe the same corpus.
    #[test]
    fn the_aql_pack_seeds_the_community_population() -> Result<(), BenchError> {
        let deck = load("aql-mix")?;
        assert_eq!(deck.id, AQL_MIX);
        assert_eq!(deck.version, "1.0.0");
        assert_eq!(deck.fixture_pins(), community_vitals().fixture_pins());
        assert_eq!(
            deck.fixture_pins()
                .get("vital_signs.opt")
                .map(String::as_str),
            Some("3a0d31bd3b5dc6329e53c0d6f22fdbaece62c684136b86139d0729cff8796128")
        );
        deck.verify_pins()
    }

    /// The AQL pack pins its population and its measured window, because both
    /// are part of what its numbers mean.
    #[test]
    fn the_aql_pack_pins_its_population_and_window() {
        let deck = aql_mix();
        assert_eq!(deck.phases.len(), 2);
        assert!(deck.sweep_phases().is_empty());
        let seeds: Vec<&SeedPhase> = deck
            .phases
            .iter()
            .filter_map(|phase| match phase {
                BenchPhase::Seed(seed) => Some(seed),
                BenchPhase::Sweep(_) | BenchPhase::Measure(_) => None,
            })
            .collect();
        let Some(seed) = seeds.first() else {
            panic!("the seed phase is gone");
        };
        assert_eq!(seed.ehrs, 50);
        assert_eq!(seed.compositions_per_ehr, 20);
        assert_eq!(seed.workers, 8);
        assert_eq!(seed.ehrs.saturating_mul(seed.compositions_per_ehr), 1_000);

        let Some(measure) = deck.measure_phases().first().copied() else {
            panic!("the measured query phase is gone");
        };
        assert_eq!(measure.name, "queries");
        assert!((measure.rate_per_s - 24.0).abs() < f64::EPSILON);
        assert_eq!(measure.warmup_s, 15);
        assert_eq!(measure.duration_s, 60);
    }

    /// The six classes are offered at equal share, every one is an ad-hoc
    /// query, and no class is repeated.
    #[test]
    fn the_aql_pack_offers_six_query_classes_at_equal_share() {
        let deck = aql_mix();
        let Some(measure) = deck.measure_phases().first().copied() else {
            panic!("the measured query phase is gone");
        };
        assert_eq!(measure.mix.len(), 6);
        assert_eq!(measure.total_share(), 6);
        assert!(
            measure.mix.iter().all(|entry| entry.share == 1),
            "a class was given an unequal share"
        );
        assert!(
            measure.mix.iter().all(|entry| entry.op.is_adhoc_query()),
            "a class is not an ad-hoc query"
        );
        let distinct: std::collections::BTreeSet<BenchOp> =
            measure.mix.iter().map(|entry| entry.op).collect();
        assert_eq!(distinct.len(), 6, "a class is repeated");
        let picked: std::collections::BTreeSet<BenchOp> = (0..6)
            .filter_map(|draw| measure.op_for_draw(draw))
            .collect();
        assert_eq!(picked, distinct, "the picker starves a class");
    }

    /// The pack description names every class from the one table that defines
    /// them, so the record a reader receives is self-describing.
    #[test]
    fn the_aql_pack_description_names_every_class() {
        let deck = aql_mix();
        for (op, rationale) in AQL_MIX_CLASSES {
            assert!(deck.description.contains(op.as_str()), "{op} is unnamed");
            assert!(
                deck.description.contains(rationale),
                "{op} lost its rationale"
            );
            assert_eq!(
                deck.probe_rationales().get(op.as_str()).map(String::as_str),
                Some(*rationale)
            );
        }
        assert!(deck.description.contains("sized for query shapes"));
    }
}
