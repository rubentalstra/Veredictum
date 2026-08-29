// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The pack manifest: what every embedded pack actually creates, measures and
//! offers, as one emitted document.
//!
//! A bench pack is versioned data compiled into this binary, so the binary is
//! the only honest source for a description of it. Everything here is derived
//! from [`crate::bench::pack`] at emission time: the phases with their
//! discipline, the operation mix with each entry's probe rationale, the
//! fixture pins with their provenance, the seed the arrival streams draw
//! from, and the requirements a record meets before it may be ranked. Nothing
//! is hand-written twice, so a rendered view over this document cannot
//! disagree with what a run executes.
//!
//! Every collection is ordered — the packs by id, the fixtures and phases in
//! the pack's own order, the rationales in a [`BTreeMap`] — so the emitted
//! bytes are a function of the pack definitions alone.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::bench::BenchError;
use crate::bench::pack::{
    self, BenchOp, BenchPack, BenchPhase, Fixture, MeasurePhase, SeedPhase, SweepPhase,
};
use crate::bench::posture::{PostureItem, PostureProfile};
use crate::bench::relative::RELATIVE_DERIVATION;
use crate::bench::result::{LoopRegime, SubmissionRequirement};

/// The file name the manifest is written under.
pub const MANIFEST_FILE: &str = "bench-packs.json";

/// What the seed disclosed in a manifest and in every record governs.
pub const SEED_DISCLOSURE: &str = "Every pack declares one seed, and every draw a run makes derives from it: which operation each arrival offers, which EHR or composition it addresses, and the query parameters a measured AQL class substitutes. Two runs of the same pack version at the same scale therefore offer the same work in the same order, on any machine and against any system, which is what makes two records comparable at all. The seed is a property of the pack rather than of the run, so it is not an operator input and no command-line flag moves it.";

/// What a posture profile is, and what the canaries around it do.
pub const POSTURE_DISCLOSURE: &str = "Two speed numbers are comparable only when the same features were switched on behind them, so every pack defines named posture profiles and a run declares exactly one with --posture. The record then carries one line per disclosed item, and each line is labelled verified or declared-only: a black-box canary reads the item off the running system before and after the measured window, and where released ITS-REST surfaces nothing to read the declaration is carried as a claim and says so. A canary that contradicts the declaration, and a pair of brackets that disagree with each other, both refuse the whole run. Authentication and TLS are facts of the invocation rather than choices of the profile, so a profile leaves them to the run.";

/// One submission requirement, as the manifest states it.
#[derive(Debug, Clone, Serialize)]
pub struct RequirementDescription {
    /// The token the record names the requirement by.
    pub token: String,
    /// What the requirement asks for, and why.
    pub statement: String,
}

impl RequirementDescription {
    /// Describes one requirement of the closed vocabulary.
    #[must_use]
    pub fn of(requirement: SubmissionRequirement) -> Self {
        Self {
            token: requirement.as_str().to_owned(),
            statement: requirement.statement().to_owned(),
        }
    }
}

/// One posture profile a pack defines: what a run declares it was measured
/// under, item by item.
///
/// The default flag names the profile a run takes when `--posture` is omitted,
/// which is the pack's first. An item the profile leaves to the invocation
/// (authentication and TLS) is absent here and supplied by the run.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileDescription {
    /// The name `--posture` accepts.
    pub name: String,
    /// What the profile switches on, in one sentence.
    pub summary: String,
    /// Whether a run that names no profile takes this one.
    pub default: bool,
    /// The declared value of each item the profile settles, keyed by the item
    /// token.
    pub declares: BTreeMap<String, String>,
}

impl ProfileDescription {
    /// Describes one profile of a pack.
    #[must_use]
    pub fn of(profile: &PostureProfile, default: bool) -> Self {
        Self {
            name: profile.name.to_owned(),
            summary: profile.summary.to_owned(),
            default,
            declares: PostureItem::ALL
                .iter()
                .copied()
                .filter_map(|item| {
                    profile
                        .declared(item)
                        .map(|value| (item.as_str().to_owned(), value.to_owned()))
                })
                .collect(),
        }
    }
}

/// One operation of the closed vocabulary, spelled out once for the whole
/// manifest so a pack's mix can name it by token alone.
#[derive(Debug, Clone, Serialize)]
pub struct OperationDescription {
    /// The token every pack, every result and every rendered table uses.
    pub token: String,
    /// The request the operation puts on the wire, as method plus a path
    /// template.
    pub wire: String,
}

impl OperationDescription {
    /// Describes one operation of the vocabulary.
    #[must_use]
    pub fn of(op: BenchOp) -> Self {
        Self {
            token: op.as_str().to_owned(),
            wire: op.wire().to_owned(),
        }
    }
}

/// One embedded fixture, as the manifest states it.
#[derive(Debug, Clone, Serialize)]
pub struct FixtureDescription {
    /// The key the pin is recorded under in every result.
    pub key: String,
    /// What the fixture is: an operational template or a composition.
    pub kind: String,
    /// The media type it goes on the wire as.
    pub media_type: String,
    /// How many bytes are embedded.
    pub bytes: usize,
    /// The lowercase-hex sha256 the bytes hash to, verified at load.
    pub sha256: String,
    /// Where the bytes came from.
    pub provenance: String,
}

impl FixtureDescription {
    /// Describes one embedded fixture.
    #[must_use]
    pub fn of(fixture: &Fixture) -> Self {
        Self {
            key: fixture.key.as_str().to_owned(),
            kind: fixture.kind.as_str().to_owned(),
            media_type: fixture.kind.media_type().to_owned(),
            bytes: fixture.bytes.len(),
            sha256: fixture.sha256.to_owned(),
            provenance: fixture.provenance.to_owned(),
        }
    }
}

/// One entry of a measured phase's operation mix, as the manifest states it.
#[derive(Debug, Clone, Serialize)]
pub struct MixDescription {
    /// The operation token.
    pub op: String,
    /// This entry's share of the arrivals, relative to every other entry's.
    pub share: u32,
    /// The arrivals per second that share works out to.
    pub rate_per_s: f64,
    /// What offering this operation probes.
    pub rationale: String,
}

/// The bulk load that builds the population every later phase reads.
#[derive(Debug, Clone, Serialize)]
pub struct SeedPhaseDescription {
    /// The phase name, as every result records it.
    pub name: String,
    /// The load regime the phase runs under.
    pub discipline: LoopRegime,
    /// How many EHRs the phase creates at scale 1.0.
    pub ehrs: usize,
    /// How many compositions it commits into each.
    pub compositions_per_ehr: usize,
    /// The population that leaves behind.
    pub compositions: usize,
    /// The closed worker pool the load runs on.
    pub workers: usize,
    /// The fixture keys the phase offers, in offer order.
    pub fixtures: Vec<String>,
}

/// The sequential walk that reproduces a single-client harness.
#[derive(Debug, Clone, Serialize)]
pub struct SweepPhaseDescription {
    /// The phase name, as every result records it.
    pub name: String,
    /// The load regime the phase runs under.
    pub discipline: LoopRegime,
    /// The closed worker pool the walk runs on.
    pub workers: usize,
    /// How many requests the walk issues against each seeded composition.
    pub requests_per_composition: usize,
    /// The operation tokens it issues, in walk order.
    pub operations: Vec<String>,
}

/// The open-loop phase the coordinated-omission-free percentiles come from.
#[derive(Debug, Clone, Serialize)]
pub struct MeasurePhaseDescription {
    /// The phase name, as every result records it.
    pub name: String,
    /// The load regime the phase runs under.
    pub discipline: LoopRegime,
    /// The aggregate arrival rate the pack version pins.
    pub rate_per_s: f64,
    /// Warmup seconds, dispatched and then discarded.
    pub warmup_s: u64,
    /// The measured span, in seconds.
    pub duration_s: u64,
    /// How many arrivals the schedule plans in total, warmup included.
    pub planned_arrivals: u64,
    /// How many of those land inside the measured window.
    pub planned_measured_arrivals: u64,
    /// The operation mix, in the pack's own order.
    pub mix: Vec<MixDescription>,
}

/// One phase of a pack, tagged by what kind of phase it is.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PhaseDescription {
    /// A closed-loop bulk load.
    Seed(SeedPhaseDescription),
    /// A closed-loop sequential walk.
    Sweep(SweepPhaseDescription),
    /// An open-loop measured phase.
    Measure(MeasurePhaseDescription),
}

impl PhaseDescription {
    /// Describes one phase of a pack.
    #[must_use]
    pub fn of(phase: &BenchPhase) -> Self {
        match phase {
            BenchPhase::Seed(seed) => PhaseDescription::Seed(Self::seed(seed)),
            BenchPhase::Sweep(sweep) => PhaseDescription::Sweep(Self::sweep(sweep)),
            BenchPhase::Measure(measure) => PhaseDescription::Measure(Self::measure(measure)),
        }
    }

    /// Describes a bulk load.
    fn seed(phase: &SeedPhase) -> SeedPhaseDescription {
        SeedPhaseDescription {
            name: phase.name.clone(),
            discipline: LoopRegime::ClosedLoop,
            ehrs: phase.ehrs,
            compositions_per_ehr: phase.compositions_per_ehr,
            compositions: phase.ehrs.saturating_mul(phase.compositions_per_ehr),
            workers: phase.workers,
            fixtures: phase
                .fixtures
                .iter()
                .map(|fixture| fixture.key.as_str().to_owned())
                .collect(),
        }
    }

    /// Describes a sequential walk.
    fn sweep(phase: &SweepPhase) -> SweepPhaseDescription {
        SweepPhaseDescription {
            name: phase.name.clone(),
            discipline: LoopRegime::ClosedLoop,
            workers: phase.workers,
            requests_per_composition: phase.per_composition.len(),
            operations: phase
                .per_composition
                .iter()
                .map(|op| op.as_str().to_owned())
                .collect(),
        }
    }

    /// Describes an open-loop measured phase.
    fn measure(phase: &MeasurePhase) -> MeasurePhaseDescription {
        MeasurePhaseDescription {
            name: phase.name.clone(),
            discipline: LoopRegime::OpenLoop,
            rate_per_s: phase.rate_per_s,
            warmup_s: phase.warmup_s,
            duration_s: phase.duration_s,
            planned_arrivals: phase.planned_arrivals(),
            planned_measured_arrivals: phase.planned_measured_arrivals(),
            mix: phase
                .mix
                .iter()
                .map(|entry| MixDescription {
                    op: entry.op.as_str().to_owned(),
                    share: entry.share,
                    rate_per_s: phase.rate_of(entry),
                    rationale: entry.rationale.clone(),
                })
                .collect(),
        }
    }
}

/// One embedded pack, as the manifest states it.
#[derive(Debug, Clone, Serialize)]
pub struct PackDescription {
    /// The pack id, which is also the `--pack` token.
    pub id: String,
    /// The pack version. Two results are comparable only when it matches.
    pub version: String,
    /// What the pack exercises, in the pack's own words.
    pub description: String,
    /// The largest share of one operation's arrivals that may fail, in one
    /// repetition of one phase, before a record stops being rankable.
    pub max_failed_share: f64,
    /// The seed every arrival stream draws from.
    pub seed: u64,
    /// The embedded fixtures, in offer order.
    pub fixtures: Vec<FixtureDescription>,
    /// The phases, in execution order.
    pub phases: Vec<PhaseDescription>,
    /// The posture profiles this pack version defines, in declaration order.
    /// A run declares exactly one of them.
    pub profiles: Vec<ProfileDescription>,
    /// What every measured operation probes, keyed by the operation token.
    pub probe_rationales: BTreeMap<String, String>,
}

impl PackDescription {
    /// Describes one loaded pack.
    #[must_use]
    pub fn of(pack: &BenchPack) -> Self {
        Self {
            id: pack.id.as_str().to_owned(),
            version: pack.version.clone(),
            description: pack.description.clone(),
            max_failed_share: pack.max_failed_share,
            seed: pack.seed,
            fixtures: pack.fixtures().iter().map(FixtureDescription::of).collect(),
            phases: pack.phases.iter().map(PhaseDescription::of).collect(),
            profiles: pack
                .profiles
                .iter()
                .enumerate()
                .map(|(index, profile)| ProfileDescription::of(profile, index == 0))
                .collect(),
            probe_rationales: pack.probe_rationales(),
        }
    }
}

/// Every embedded pack, with the disciplines and rules a reader needs to read
/// a bench record.
#[derive(Debug, Clone, Serialize)]
pub struct PackManifest {
    /// The published schema version this document conforms to.
    pub schema_version: String,
    /// What a bench result is, and what it is never.
    pub boundary_statement: String,
    /// The methodology every bench run follows.
    pub methodology: String,
    /// How the relative index on a board row was derived.
    pub relative_index: String,
    /// What the seed governs, and why it is not an operator input.
    pub seed_disclosure: String,
    /// What a posture profile is, and how its canaries check it.
    pub posture_disclosure: String,
    /// What a record must carry before it may be offered for ranking.
    pub submission_requirements: Vec<RequirementDescription>,
    /// Every operation a pack may offer, in token order.
    pub operations: Vec<OperationDescription>,
    /// The embedded packs, in the order `--pack` accepts them.
    pub packs: Vec<PackDescription>,
}

impl PackManifest {
    /// Describes every pack this binary embeds, verifying each one's fixture
    /// pins on the way.
    ///
    /// # Errors
    /// [`BenchError::FixturePin`] when an embedded fixture's bytes moved, and
    /// [`BenchError::UnknownPack`] when the embedded id list names a pack the
    /// loader does not answer to.
    pub fn of_embedded() -> Result<Self, BenchError> {
        let mut packs = Vec::with_capacity(pack::EMBEDDED.len());
        for id in pack::EMBEDDED {
            packs.push(PackDescription::of(&pack::load(id.as_str())?));
        }
        Ok(Self {
            schema_version: crate::schema::SCHEMA_VERSION.to_owned(),
            boundary_statement: crate::bench::BOUNDARY_STATEMENT.to_owned(),
            methodology: crate::bench::METHODOLOGY.to_owned(),
            relative_index: RELATIVE_DERIVATION.to_owned(),
            seed_disclosure: SEED_DISCLOSURE.to_owned(),
            posture_disclosure: POSTURE_DISCLOSURE.to_owned(),
            submission_requirements: SubmissionRequirement::ALL
                .iter()
                .copied()
                .map(RequirementDescription::of)
                .collect(),
            operations: BenchOp::ALL
                .iter()
                .copied()
                .map(OperationDescription::of)
                .collect(),
            packs,
        })
    }

    /// The document's canonical text: two-space pretty print, trailing
    /// newline, exactly as every other emitted artifact family.
    ///
    /// # Errors
    /// [`BenchError::Serialize`] when the value cannot be serialized.
    pub fn to_document(&self) -> Result<String, BenchError> {
        let mut text =
            serde_json::to_string_pretty(self).map_err(|source| BenchError::Serialize {
                context: "bench pack manifest",
                source,
            })?;
        text.push('\n');
        Ok(text)
    }
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// The manifest describes every embedded pack, with nothing left blank.
    #[test]
    fn the_manifest_describes_every_embedded_pack() -> Result<(), BenchError> {
        let manifest = PackManifest::of_embedded()?;
        assert_eq!(manifest.packs.len(), pack::EMBEDDED.len());
        for (description, id) in manifest.packs.iter().zip(pack::EMBEDDED) {
            assert_eq!(description.id, id.as_str());
            assert!(!description.version.is_empty(), "{id} has no version");
            assert!(!description.phases.is_empty(), "{id} declares no phase");
            assert!(!description.fixtures.is_empty(), "{id} embeds no fixture");
            for fixture in &description.fixtures {
                assert!(
                    !fixture.provenance.trim().is_empty(),
                    "{id}: {} states no provenance",
                    fixture.key
                );
                assert_eq!(fixture.sha256.len(), 64, "{id}: {}", fixture.key);
            }
        }
        assert!(!manifest.submission_requirements.is_empty());
        Ok(())
    }

    /// Every pack publishes its posture profiles with exactly one default, and
    /// no profile declares an item the run supplies.
    #[test]
    fn every_pack_publishes_its_posture_profiles() -> Result<(), BenchError> {
        let manifest = PackManifest::of_embedded()?;
        for description in &manifest.packs {
            assert!(
                !description.profiles.is_empty(),
                "{}: no posture profile",
                description.id
            );
            let defaults = description
                .profiles
                .iter()
                .filter(|profile| profile.default)
                .count();
            assert_eq!(defaults, 1, "{}: not exactly one default", description.id);
            for profile in &description.profiles {
                assert!(!profile.summary.trim().is_empty(), "{}", profile.name);
                assert!(
                    !profile.declares.contains_key(PostureItem::Authn.as_str()),
                    "{}: authentication is an invocation fact",
                    profile.name
                );
                assert!(
                    !profile.declares.contains_key(PostureItem::Tls.as_str()),
                    "{}: TLS is an invocation fact",
                    profile.name
                );
                assert!(!profile.declares.is_empty(), "{}", profile.name);
            }
        }
        Ok(())
    }

    /// Emission is byte-deterministic, which is what makes the rendered legend
    /// diffable against the packs it came from.
    #[test]
    fn emission_is_byte_deterministic() -> Result<(), BenchError> {
        let first = PackManifest::of_embedded()?.to_document()?;
        let second = PackManifest::of_embedded()?.to_document()?;
        assert_eq!(first, second);
        assert!(first.ends_with('\n'));
        Ok(())
    }

    /// Every measured phase states its planned arrival counts, and the
    /// measured window is the smaller of the two.
    #[test]
    fn a_measured_phase_states_its_planned_arrivals() -> Result<(), BenchError> {
        let manifest = PackManifest::of_embedded()?;
        let mut measured = 0_usize;
        for description in &manifest.packs {
            for phase in &description.phases {
                let PhaseDescription::Measure(measure) = phase else {
                    continue;
                };
                measured = measured.saturating_add(1);
                assert_eq!(measure.discipline, LoopRegime::OpenLoop);
                assert!(measure.planned_arrivals > 0, "{}", description.id);
                assert!(
                    measure.planned_measured_arrivals < measure.planned_arrivals,
                    "{}: the warmup discards nothing",
                    description.id
                );
                let offered: f64 = measure.mix.iter().map(|entry| entry.rate_per_s).sum();
                assert!(
                    (offered - measure.rate_per_s).abs() < 1e-9,
                    "{}: the per-operation rates do not sum to the phase rate",
                    description.id
                );
            }
        }
        assert!(measured >= 3, "a pack lost its measured phase");
        Ok(())
    }

    /// The community pack's read walk is closed-loop and its open-loop half is
    /// not, because the whole point of carrying both is that they are read
    /// differently.
    #[test]
    fn the_two_disciplines_are_labelled_apart() -> Result<(), BenchError> {
        let manifest = PackManifest::of_embedded()?;
        let Some(community) = manifest
            .packs
            .iter()
            .find(|description| description.id == "community-vitals")
        else {
            panic!("the community pack is gone");
        };
        let disciplines: Vec<LoopRegime> = community
            .phases
            .iter()
            .map(|phase| match phase {
                PhaseDescription::Seed(seed) => seed.discipline,
                PhaseDescription::Sweep(sweep) => sweep.discipline,
                PhaseDescription::Measure(measure) => measure.discipline,
            })
            .collect();
        assert_eq!(
            disciplines,
            vec![
                LoopRegime::ClosedLoop,
                LoopRegime::ClosedLoop,
                LoopRegime::OpenLoop
            ]
        );
        Ok(())
    }
}
