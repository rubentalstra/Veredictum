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

use serde::Deserialize;
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

/// Where [`BLOOD_PRESSURE_OPT`] comes from.
const BLOOD_PRESSURE_OPT_PROVENANCE: &str = "\
Authored in this repository for the smoke pack: an ADL 1.4 operational \
template with template id 'cnf.blood_pressure', rooted at \
openEHR-EHR-COMPOSITION.minimal.v1 and constraining \
openEHR-EHR-OBSERVATION.blood_pressure.v2. It exists to give the smoke pack a \
small upload, and it is not derived from any published library.";

/// One canonical-JSON `COMPOSITION` constrained by [`BLOOD_PRESSURE_OPT`]:
/// a single `POINT_EVENT` carrying a systolic and a diastolic
/// `DV_QUANTITY` in `mm[Hg]` under the
/// `openEHR-EHR-OBSERVATION.blood_pressure.v2` archetype (openEHR RM
/// `data_structures` §`HISTORY`/`POINT_EVENT`).
///
/// Its `archetype_node_id` and `archetype_details.archetype_id` both carry
/// `openEHR-EHR-COMPOSITION.minimal.v1`, the archetype the named template
/// roots at, which is what [`BenchPack::verify_fixture_roots`] proves at every
/// load.
const BP_COMPOSITION: &str = include_str!("fixtures/bp_composition.json");

/// The pinned digest of [`BP_COMPOSITION`].
const BP_COMPOSITION_SHA256: &str =
    "bc0d07f4a6f89e5b357cddd558b4c05a6ee9cd4083dda6646e1b23ee80ff1d47";

/// Where [`BP_COMPOSITION`] comes from.
const BP_COMPOSITION_PROVENANCE: &str = "\
Authored in this repository for the smoke pack: a canonical-JSON COMPOSITION \
declaring template id 'cnf.blood_pressure' and rooted at \
openEHR-EHR-COMPOSITION.minimal.v1, the root that template defines, carrying \
one POINT_EVENT with a systolic and a diastolic DV_QUANTITY in mm[Hg].";

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
    "eaec78e4b3541189b63bc2a83cbff88e4727aa3893406e08777e7330cbfb72b6";

/// Where [`BP_COMPOSITION_TWIN`] comes from.
const BP_COMPOSITION_TWIN_PROVENANCE: &str = "\
Derived in this repository from bp_composition.json by deleting the mandatory \
COMPOSITION.composer member and nothing else, so a server that validates a \
commit against the reference model refuses it.";

/// The `Vital signs` operational template the community harness uploads,
/// embedded byte-identically from the vendored CKM template pack (CKM cid
/// 1013.26.380; template id `Vital signs`, root
/// `openEHR-EHR-COMPOSITION.encounter.v1`).
const VITAL_SIGNS_OPT: &str = include_str!("fixtures/vital_signs.opt");

/// The pinned digest of [`VITAL_SIGNS_OPT`].
const VITAL_SIGNS_OPT_SHA256: &str =
    "3a0d31bd3b5dc6329e53c0d6f22fdbaece62c684136b86139d0729cff8796128";

/// Where [`VITAL_SIGNS_OPT`] comes from.
const VITAL_SIGNS_OPT_PROVENANCE: &str = "\
The openEHR Clinical Knowledge Manager's own Operational Template export for \
template id 'Vital signs' (CKM cid 1013.26.380, <https://ckm.openehr.org/ckm>), \
vendored byte-identically and rooted at openEHR-EHR-COMPOSITION.encounter.v1.";

/// The `Vital signs` `COMPOSITION` instance the community harness commits,
/// byte-identical to the attachment on post 8 of
/// <https://discourse.openehr.org/t/17224>: eight `OBSERVATION` entries under
/// `openEHR-EHR-COMPOSITION.encounter.v1`, `rm_version` 1.0.2.
const VITAL_SIGNS_COMPOSITION: &str = include_str!("fixtures/vital_signs_composition.json");

/// The pinned digest of [`VITAL_SIGNS_COMPOSITION`].
const VITAL_SIGNS_COMPOSITION_SHA256: &str =
    "468081c259c737d35d7f80403562b3f333e479d267286faf80fd7c087eaba947";

/// Where [`VITAL_SIGNS_COMPOSITION`] comes from.
const VITAL_SIGNS_COMPOSITION_PROVENANCE: &str = "\
The composition attached to post 8 of the openEHR community's vital-signs \
benchmark thread (<https://discourse.openehr.org/t/17224>), vendored \
byte-identically: eight OBSERVATION entries under \
openEHR-EHR-COMPOSITION.encounter.v1, rm_version 1.0.2, declaring template id \
'Vital signs'.";

/// The invalid twin of [`VITAL_SIGNS_COMPOSITION`], derived the same way
/// [`BP_COMPOSITION_TWIN`] is: the mandatory `COMPOSITION.composer` member
/// deleted and nothing else changed.
const VITAL_SIGNS_COMPOSITION_TWIN: &str =
    include_str!("fixtures/vital_signs_composition.missing_composer.json");

/// The pinned digest of [`VITAL_SIGNS_COMPOSITION_TWIN`].
const VITAL_SIGNS_COMPOSITION_TWIN_SHA256: &str =
    "f0598db5ab447b371ead28cba0f841f72370dbbf93db98d5b8e477910a42688d";

/// Where [`VITAL_SIGNS_COMPOSITION_TWIN`] comes from.
const VITAL_SIGNS_COMPOSITION_TWIN_PROVENANCE: &str = "\
Derived in this repository from vital_signs_composition.json by deleting the \
mandatory COMPOSITION.composer member and nothing else, so a server that \
validates a commit against the reference model refuses it.";

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
    /// Every kind, in the order the emitted manifest's schema enumerates them.
    pub const ALL: &[FixtureKind] = &[
        FixtureKind::Composition,
        FixtureKind::InvalidComposition,
        FixtureKind::OperationalTemplate,
    ];

    /// The token an emitted document records the kind under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            FixtureKind::OperationalTemplate => "operational_template",
            FixtureKind::Composition => "composition",
            FixtureKind::InvalidComposition => "invalid_composition",
        }
    }

    /// The media type the fixture goes on the wire as.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            FixtureKind::OperationalTemplate => "application/xml",
            FixtureKind::Composition | FixtureKind::InvalidComposition => "application/json",
        }
    }
}

impl fmt::Display for FixtureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    /// Where the bytes came from, in one sentence, so a reader of the emitted
    /// manifest can go and fetch the source material.
    ///
    /// The digest above is what makes this checkable: the provenance names the
    /// source, and re-hashing the source is how a reader confirms it. Editing
    /// this string moves no byte the pack offers, so it does not bump the pack
    /// version.
    pub provenance: &'static str,
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

    /// The request the operation puts on the wire, as method plus a path
    /// template over the four values an arrival substitutes: `{ehr_id}`,
    /// `{uid}`, `{version_uid}` and `{at_time}`.
    ///
    /// The dispatcher builds every offered path from this template, so what an
    /// emitted manifest publishes is the request that actually goes out.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            BenchOp::CreateComposition => "POST /ehr/{ehr_id}/composition",
            BenchOp::GetCompositionAtTime => {
                "GET /ehr/{ehr_id}/composition/{uid}?version_at_time={at_time}"
            }
            BenchOp::GetCompositionLatest => "GET /ehr/{ehr_id}/composition/{uid}",
            BenchOp::GetEhr => "GET /ehr/{ehr_id}",
            BenchOp::GetEhrStatus => "GET /ehr/{ehr_id}/ehr_status",
            BenchOp::GetVersionedComposition => "GET /ehr/{ehr_id}/versioned_composition/{uid}",
            BenchOp::GetVersionedCompositionRevisionHistory => {
                "GET /ehr/{ehr_id}/versioned_composition/{uid}/revision_history"
            }
            BenchOp::GetVersionedCompositionVersionAtTime => {
                "GET /ehr/{ehr_id}/versioned_composition/{uid}/version?version_at_time={at_time}"
            }
            BenchOp::GetVersionedCompositionVersionById => {
                "GET /ehr/{ehr_id}/versioned_composition/{uid}/version/{version_uid}"
            }
            BenchOp::GetVersionedCompositionVersionLatest => {
                "GET /ehr/{ehr_id}/versioned_composition/{uid}/version"
            }
            BenchOp::AdhocQueryUid
            | BenchOp::AdhocQueryAggregate
            | BenchOp::AdhocQueryEhrScan
            | BenchOp::AdhocQueryFiltered
            | BenchOp::AdhocQueryOrderedPage
            | BenchOp::AdhocQueryPointLookup
            | BenchOp::AdhocQueryPopulation => "POST /query/aql",
        }
    }

    /// The path half of [`BenchOp::wire`], with every placeholder replaced by
    /// the value this arrival addresses.
    #[must_use]
    pub fn path(self, ehr_id: &str, uid: &str, version_uid: &str, at_time: &str) -> String {
        self.wire()
            .split_once(' ')
            .map_or(self.wire(), |(_method, path)| path)
            .replace("{ehr_id}", ehr_id)
            .replace("{uid}", uid)
            .replace("{version_uid}", version_uid)
            .replace("{at_time}", at_time)
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

    /// How many arrivals the phase's schedule plans, warmup included.
    ///
    /// The dispatcher builds its schedule from this count, and the emitted
    /// pack manifest prints it, so the two can never describe different work.
    #[must_use]
    pub fn planned_arrivals(&self) -> u64 {
        let span_s = self.warmup_s.saturating_add(self.duration_s);
        if self.rate_per_s <= 0.0 || span_s == 0 {
            return 0;
        }
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the arrival count is rate x span, both operator-scale values far below 2^52"
        )]
        let total = (self.rate_per_s * span_s as f64).ceil() as u64;
        total
    }

    /// Whether the arrival at `index` falls inside the measured window rather
    /// than the warmup that precedes it.
    #[must_use]
    pub fn is_measured(&self, index: u64) -> bool {
        if self.rate_per_s <= 0.0 {
            return false;
        }
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            reason = "the arrival ordinal and the warmup boundary are operator-scale values far below 2^52"
        )]
        let measured = index as f64 / self.rate_per_s >= self.warmup_s as f64;
        measured
    }

    /// How many of the planned arrivals land inside the measured window.
    #[must_use]
    pub fn planned_measured_arrivals(&self) -> u64 {
        (0..self.planned_arrivals())
            .filter(|index| self.is_measured(*index))
            .count()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    /// The arrivals per second one mix entry is offered at, given its share of
    /// the phase's aggregate rate.
    #[must_use]
    pub fn rate_of(&self, entry: &MixEntry) -> f64 {
        let total = self.total_share();
        if total == 0 {
            return 0.0;
        }
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            reason = "the share and its total are small operator-scale counts"
        )]
        let rate = self.rate_per_s * (f64::from(entry.share) / total as f64);
        rate
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

/// The failed-arrival ceiling every embedded pack pins.
///
/// Deliberately conservative: one arrival in a hundred is already more than a
/// system answering its own pinned load should lose, and a record above it
/// reports percentiles taken over failures. No openEHR spec governs this — our
/// own design.
pub const DEFAULT_MAX_FAILED_SHARE: f64 = 0.01;

/// The sentence every embedded pack's description carries about its ceiling.
///
/// It is built from [`DEFAULT_MAX_FAILED_SHARE`] itself, so the number a
/// description states and the number the engine judges by cannot drift.
#[must_use]
pub fn failed_share_statement() -> String {
    format!(
        "This pack version pins a failed-arrival ceiling of {DEFAULT_MAX_FAILED_SHARE:.2}: a \
         record in which any repetition, phase and operation loses a larger share of its \
         arrivals, on the target or on any baseline, is not submittable, because percentiles \
         taken over failed arrivals measure the failure rather than the system."
    )
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
    /// The largest share of one operation's arrivals that may fail, in one
    /// repetition of one phase, before the record stops being rankable.
    ///
    /// Pack-wide rather than per phase: the ceiling says when a measurement
    /// stopped describing the system, which is the same question in a measured
    /// phase, in a sweep, and in a baseline's copy of both. It is part of the
    /// versioned pack definition and the result discloses it, so a reader
    /// always sees the rule a record was judged by.
    pub max_failed_share: f64,
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

    /// Verifies that every composition fixture declares the root archetype the
    /// template it names actually roots at.
    ///
    /// A `COMPOSITION` carries its root twice: `archetype_node_id`, which "at
    /// an archetype root point … is always the stringified form of the
    /// `_archetype_id_` found in the `_archetype_details_` object" (openEHR RM
    /// `UML/classes/locatable.adoc` §Attributes), and that
    /// `archetype_details.archetype_id` itself, beside the
    /// `archetype_details.template_id` naming the template active at this
    /// point in the structure (openEHR RM `UML/classes/archetyped.adoc`
    /// §Attributes). The operational template that id resolves to declares the
    /// archetype its own `definition` roots at, so both declared ids must be
    /// that root. A pack whose fixture declares another one measures a server's
    /// refusals where it is validated, and a server's leniency where it is not.
    /// The check runs at load, beside the digest pins.
    ///
    /// # Errors
    /// [`BenchError::FixtureUnreadable`] when a fixture does not parse or
    /// omits an id this check reads, [`BenchError::FixtureTemplate`] when a
    /// composition names a template the pack does not seed, and
    /// [`BenchError::FixtureRoot`] naming both declared ids and the template's
    /// root when they differ.
    pub fn verify_fixture_roots(&self) -> Result<(), BenchError> {
        let templates = self.template_identities()?;
        for fixture in self.fixtures() {
            match fixture.kind {
                FixtureKind::OperationalTemplate => continue,
                FixtureKind::Composition | FixtureKind::InvalidComposition => {}
            }
            let declared = composition_roots(fixture.bytes).map_err(|detail| {
                BenchError::FixtureUnreadable {
                    pack: self.id,
                    fixture: fixture.key,
                    detail,
                }
            })?;
            let Some(template) = templates
                .iter()
                .find(|identity| identity.id == declared.archetype_details.template_id.value)
            else {
                return Err(BenchError::FixtureTemplate {
                    pack: self.id,
                    fixture: fixture.key,
                    template: declared.archetype_details.template_id.value,
                    seeded: templates
                        .iter()
                        .map(|identity| identity.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            };
            if declared.archetype_node_id != template.root
                || declared.archetype_details.archetype_id.value != template.root
            {
                return Err(BenchError::FixtureRoot(Box::new(RootMismatch {
                    pack: self.id,
                    fixture: fixture.key,
                    template: template.id.clone(),
                    root: template.root.clone(),
                    node_id: declared.archetype_node_id,
                    archetype_id: declared.archetype_details.archetype_id.value,
                })));
            }
        }
        Ok(())
    }

    /// The identity every operational template the pack seeds declares.
    ///
    /// # Errors
    /// [`BenchError::FixtureUnreadable`] when a template does not parse or
    /// declares no template id or no root archetype id.
    fn template_identities(&self) -> Result<Vec<TemplateIdentity>, BenchError> {
        let mut identities = Vec::new();
        for fixture in self.fixtures() {
            match fixture.kind {
                FixtureKind::OperationalTemplate => {}
                FixtureKind::Composition | FixtureKind::InvalidComposition => continue,
            }
            let identity = template_identity(fixture.bytes).map_err(|detail| {
                BenchError::FixtureUnreadable {
                    pack: self.id,
                    fixture: fixture.key,
                    detail,
                }
            })?;
            identities.push(identity);
        }
        Ok(identities)
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

/// What a composition fixture declares against what its template roots at.
///
/// Carried boxed by [`BenchError::FixtureRoot`], so the error type stays small
/// enough for every `Result` in the engine to return it by value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootMismatch {
    /// The pack carrying the fixture.
    pub pack: PackId,
    /// The composition fixture key.
    pub fixture: FixtureKey,
    /// The template id the fixture declares.
    pub template: String,
    /// The archetype that template's definition roots at.
    pub root: String,
    /// The root `archetype_node_id` the fixture declares.
    pub node_id: String,
    /// The `archetype_details.archetype_id` the fixture declares.
    pub archetype_id: String,
}

/// What an operational template declares about its own identity.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TemplateIdentity {
    /// The template id, from `template/template_id/value`.
    id: String,
    /// The archetype the template's definition roots at, from
    /// `template/definition/archetype_id/value`.
    root: String,
}

/// What one composition fixture declares about the archetype it roots at.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct CompositionRoots {
    /// The root `archetype_node_id`.
    archetype_node_id: String,
    /// The `ARCHETYPED` block naming the archetype and the template.
    archetype_details: DeclaredArchetyped,
}

/// The `archetype_details` members the coherence check reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DeclaredArchetyped {
    /// The archetype the fixture declares as its root.
    archetype_id: DeclaredId,
    /// The template the fixture declares it was built from.
    template_id: DeclaredId,
}

/// The `{ "value": … }` wrapper canonical JSON gives an identifier.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DeclaredId {
    /// The identifier itself.
    value: String,
}

/// Reads an operational template's identity out of its XML.
///
/// The read is a total pull over two known element paths, so a document that
/// is not well formed, ends with an element open, or omits either path is a
/// failure rather than a default.
fn template_identity(opt_xml: &str) -> Result<TemplateIdentity, String> {
    let mut reader = quick_xml::Reader::from_str(opt_xml);
    let mut path: Vec<String> = Vec::new();
    let mut id: Option<String> = None;
    let mut root: Option<String> = None;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            quick_xml::events::Event::Eof if path.is_empty() => break,
            quick_xml::events::Event::Eof => {
                return Err(format!(
                    "the document ends with {} element(s) still open",
                    path.len()
                ));
            }
            quick_xml::events::Event::Start(start) => {
                path.push(String::from_utf8_lossy(start.local_name().as_ref()).into_owned());
            }
            quick_xml::events::Event::End(_) => {
                path.pop();
            }
            quick_xml::events::Event::Text(text) => {
                let decoded = text.decode().map_err(|e| e.to_string())?;
                let trimmed = decoded.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if path_is(&path, &["template", "template_id", "value"]) {
                    id = Some(trimmed.to_owned());
                } else if path_is(&path, &["template", "definition", "archetype_id", "value"]) {
                    root = Some(trimmed.to_owned());
                }
            }
            _ => {}
        }
    }
    let id = id.ok_or_else(|| "no template/template_id/value".to_owned())?;
    let root = root.ok_or_else(|| "no template/definition/archetype_id/value".to_owned())?;
    Ok(TemplateIdentity { id, root })
}

/// Whether the open-element path is exactly this sequence of local names.
fn path_is(path: &[String], expected: &[&str]) -> bool {
    path.len() == expected.len()
        && path
            .iter()
            .zip(expected)
            .all(|(open, want)| open.as_str() == *want)
}

/// Reads the two root ids and the template id a composition fixture declares.
///
/// Every member this reads is mandatory on a root `COMPOSITION` the pack
/// commits, so an absent one is a deserialization failure rather than a
/// default.
fn composition_roots(json: &str) -> Result<CompositionRoots, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
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

/// Loads one embedded pack by its id, verifying every fixture pin and the
/// coherence of every fixture root with its template.
///
/// # Errors
/// [`BenchError::UnknownPack`] for a token no embedded pack answers to,
/// [`BenchError::FixturePin`] when an embedded fixture's bytes moved, or
/// whatever [`BenchPack::verify_fixture_roots`] reports.
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
    pack.verify_fixture_roots()?;
    Ok(pack)
}

/// What the `smoke` pack exercises, ahead of its ceiling statement.
const SMOKE_PREAMBLE: &str = "\
One blood-pressure template, a small EHR corpus, and a mixed open-loop phase \
over the read, write and query surface. Version 1.1.0 moved the committed \
composition onto openEHR-EHR-COMPOSITION.minimal.v1, the root its own template \
defines, so the bytes offered to a server changed and a 1.1.0 record is not \
comparable with a 1.0.0 one.";

/// The `smoke` pack: one small bulk load, then one short open-loop phase
/// over the whole operation vocabulary.
#[must_use]
pub fn smoke() -> BenchPack {
    BenchPack {
        id: SMOKE,
        version: "1.1.0".to_owned(),
        description: format!("{SMOKE_PREAMBLE} {}", failed_share_statement()),
        max_failed_share: DEFAULT_MAX_FAILED_SHARE,
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
                        provenance: BLOOD_PRESSURE_OPT_PROVENANCE,
                    },
                    Fixture {
                        key: FixtureKey("bp_composition.json"),
                        kind: FixtureKind::Composition,
                        bytes: BP_COMPOSITION,
                        sha256: BP_COMPOSITION_SHA256,
                        provenance: BP_COMPOSITION_PROVENANCE,
                    },
                    Fixture {
                        key: FixtureKey("bp_composition.missing_composer.json"),
                        kind: FixtureKind::InvalidComposition,
                        bytes: BP_COMPOSITION_TWIN,
                        sha256: BP_COMPOSITION_TWIN_SHA256,
                        provenance: BP_COMPOSITION_TWIN_PROVENANCE,
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
            provenance: VITAL_SIGNS_OPT_PROVENANCE,
        },
        Fixture {
            key: FixtureKey("vital_signs_composition.json"),
            kind: FixtureKind::Composition,
            bytes: VITAL_SIGNS_COMPOSITION,
            sha256: VITAL_SIGNS_COMPOSITION_SHA256,
            provenance: VITAL_SIGNS_COMPOSITION_PROVENANCE,
        },
        Fixture {
            key: FixtureKey("vital_signs_composition.missing_composer.json"),
            kind: FixtureKind::InvalidComposition,
            bytes: VITAL_SIGNS_COMPOSITION_TWIN,
            sha256: VITAL_SIGNS_COMPOSITION_TWIN_SHA256,
            provenance: VITAL_SIGNS_COMPOSITION_TWIN_PROVENANCE,
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
        description: format!(
            "{COMMUNITY_VITALS_DESCRIPTION} {}",
            failed_share_statement()
        ),
        max_failed_share: DEFAULT_MAX_FAILED_SHARE,
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
        max_failed_share: DEFAULT_MAX_FAILED_SHARE,
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
    format!(
        "{AQL_MIX_PREAMBLE} The six classes: {classes}. {AQL_MIX_PROVENANCE} {}",
        failed_share_statement()
    )
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

    /// A composition fixture doctored to declare a root its template does not
    /// have, for the refusal test below.
    const DOCTORED_COMPOSITION: &str = r#"{
  "_type": "COMPOSITION",
  "archetype_node_id": "openEHR-EHR-COMPOSITION.encounter.v1",
  "archetype_details": {
    "_type": "ARCHETYPED",
    "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.encounter.v1" },
    "template_id": { "_type": "TEMPLATE_ID", "value": "cnf.blood_pressure" },
    "rm_version": "1.0.2"
  }
}"#;

    /// A composition fixture naming a template no pack seeds, for the refusal
    /// test below.
    const UNSEEDED_TEMPLATE_COMPOSITION: &str = r#"{
  "_type": "COMPOSITION",
  "archetype_node_id": "openEHR-EHR-COMPOSITION.minimal.v1",
  "archetype_details": {
    "_type": "ARCHETYPED",
    "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.minimal.v1" },
    "template_id": { "_type": "TEMPLATE_ID", "value": "cnf.absent" },
    "rm_version": "1.0.2"
  }
}"#;

    /// Replaces the smoke pack's composition fixture bytes, keeping every
    /// other fixture and phase as the pack declares them.
    fn with_composition_bytes(bytes: &'static str) -> BenchPack {
        let pack = smoke();
        let phases = pack
            .phases
            .iter()
            .map(|phase| match phase {
                BenchPhase::Seed(seed) => {
                    let mut seed = seed.clone();
                    for fixture in &mut seed.fixtures {
                        if fixture.kind == FixtureKind::Composition {
                            fixture.bytes = bytes;
                        }
                    }
                    BenchPhase::Seed(seed)
                }
                other => other.clone(),
            })
            .collect();
        pack.with_phases(phases)
    }

    /// Every embedded pack's composition fixtures declare the root their own
    /// template roots at, which is what a validating server checks on commit.
    #[test]
    fn every_embedded_pack_agrees_with_its_own_templates() -> Result<(), BenchError> {
        for id in EMBEDDED {
            load(id.as_str())?.verify_fixture_roots()?;
        }
        Ok(())
    }

    /// The smoke template roots at the archetype its committed compositions
    /// declare, read out of the template itself.
    #[test]
    fn the_smoke_template_declares_the_root_its_compositions_carry() -> Result<(), String> {
        let identity = template_identity(BLOOD_PRESSURE_OPT)?;
        assert_eq!(identity.id, "cnf.blood_pressure");
        assert_eq!(identity.root, "openEHR-EHR-COMPOSITION.minimal.v1");
        for bytes in [BP_COMPOSITION, BP_COMPOSITION_TWIN] {
            let declared = composition_roots(bytes)?;
            assert_eq!(declared.archetype_details.template_id.value, identity.id);
            assert_eq!(declared.archetype_node_id, identity.root);
            assert_eq!(declared.archetype_details.archetype_id.value, identity.root);
        }
        Ok(())
    }

    /// A composition that declares another archetype's root is refused, and
    /// the refusal names both declared ids beside the template's own root.
    #[test]
    fn a_fixture_root_its_template_does_not_have_is_refused() {
        let error = with_composition_bytes(DOCTORED_COMPOSITION)
            .verify_fixture_roots()
            .unwrap_err();
        let BenchError::FixtureRoot(mismatch) = &error else {
            panic!("expected a fixture-root refusal, got {error}");
        };
        assert_eq!(mismatch.fixture.as_str(), "bp_composition.json");
        assert_eq!(mismatch.template, "cnf.blood_pressure");
        assert_eq!(mismatch.root, "openEHR-EHR-COMPOSITION.minimal.v1");
        assert_eq!(mismatch.node_id, "openEHR-EHR-COMPOSITION.encounter.v1");
        assert_eq!(
            mismatch.archetype_id,
            "openEHR-EHR-COMPOSITION.encounter.v1"
        );
        let rendered = error.to_string();
        assert!(
            rendered.contains("openEHR-EHR-COMPOSITION.encounter.v1")
                && rendered.contains("openEHR-EHR-COMPOSITION.minimal.v1"),
            "{rendered}"
        );
    }

    /// A composition naming a template the pack never seeds is refused by
    /// name, never checked against some other template's root.
    #[test]
    fn a_fixture_naming_an_unseeded_template_is_refused() {
        let error = with_composition_bytes(UNSEEDED_TEMPLATE_COMPOSITION)
            .verify_fixture_roots()
            .unwrap_err();
        assert!(
            matches!(error, BenchError::FixtureTemplate { .. }),
            "{error}"
        );
        assert!(error.to_string().contains("cnf.absent"), "{error}");
    }

    /// A fixture that is not readable as a composition is refused, never
    /// treated as coherent by default.
    #[test]
    fn an_unreadable_fixture_is_refused() {
        let error = with_composition_bytes("{")
            .verify_fixture_roots()
            .unwrap_err();
        assert!(
            matches!(error, BenchError::FixtureUnreadable { .. }),
            "{error}"
        );
    }

    /// A truncated operational template is refused rather than read as far as
    /// it goes.
    #[test]
    fn a_truncated_template_is_refused() {
        let truncated = "<template><template_id><value>t</value></template_id>";
        assert!(template_identity(truncated).is_err());
    }

    /// A moved pin is refused by key, never silently accepted.
    #[test]
    fn a_moved_pin_is_refused() {
        let fixture = Fixture {
            key: FixtureKey("moved"),
            kind: FixtureKind::Composition,
            bytes: "{}",
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            provenance: "authored for this test",
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

    /// Every operation publishes a wire template whose placeholders are all
    /// substituted, so a rendered legend never prints a `{uid}` at a reader.
    #[test]
    fn every_operation_publishes_a_substitutable_wire_template() {
        for op in BenchOp::ALL {
            let wire = op.wire();
            assert!(
                wire.starts_with("GET /") || wire.starts_with("POST /"),
                "{op}: {wire} is not a method plus a path"
            );
            let path = op.path("E", "U", "V", "T");
            assert!(!path.contains('{'), "{op}: {path} kept a placeholder");
            assert!(path.starts_with('/'), "{op}: {path} is not rooted");
        }
    }

    /// The path a composition read addresses names the object it addresses,
    /// and an EHR-scoped operation never carries a composition uid.
    #[test]
    fn a_wire_path_substitutes_only_what_its_operation_addresses() {
        assert_eq!(
            BenchOp::GetVersionedCompositionVersionById.path("E", "U", "V", "T"),
            "/ehr/E/versioned_composition/U/version/V"
        );
        assert_eq!(
            BenchOp::GetCompositionAtTime.path("E", "U", "V", "T"),
            "/ehr/E/composition/U?version_at_time=T"
        );
        assert_eq!(
            BenchOp::GetEhrStatus.path("E", "U", "V", "T"),
            "/ehr/E/ehr_status"
        );
        assert_eq!(
            BenchOp::AdhocQueryUid.path("E", "U", "V", "T"),
            "/query/aql"
        );
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

    /// Every embedded pack pins the conservative failed-arrival ceiling and
    /// states it in its own description, so a record carries the rule it was
    /// judged by without a reader consulting anything else.
    #[test]
    fn every_embedded_pack_pins_and_states_its_failed_arrival_ceiling() -> Result<(), BenchError> {
        for id in EMBEDDED {
            let deck = load(id.as_str())?;
            assert!(
                (deck.max_failed_share - DEFAULT_MAX_FAILED_SHARE).abs() < f64::EPSILON,
                "{id} pins {} rather than the conservative default",
                deck.max_failed_share
            );
            assert!(
                deck.description.contains(&failed_share_statement()),
                "{id} does not state its failed-arrival ceiling"
            );
            assert!(deck.description.contains("0.01"), "{id}");
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
