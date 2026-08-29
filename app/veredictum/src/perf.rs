// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The performance schedule machinery — conformance-by-MEASUREMENT.
//!
//! It carries the
//! `kind: performance` case model (class, corpus, open-loop workload,
//! thresholds), the journey catalogue (the hospital-simulation vocabulary a
//! workload decomposes into), the measurement record (counts, errors,
//! percentiles, the encoded HDR histogram so every threshold is
//! RE-CHECKABLE from the artifact), and the class-verdict pure function
//! (earned | not-earned).
//!
//! The class floors are the population-anchored \[legislated\] defaults the
//! schedule publishes (POC 2/s · S 15/s · L 150/s · R 1,500/s peak
//! arrivals, p99 ≤ 1 s, error rate 0) — implemented exactly as specified;
//! upstream ratification owns any change. The workload model is OPEN-LOOP
//! (a seeded arrival schedule, never closed-loop users) so coordinated
//! omission cannot hide stalls.
//!
//! NOTE: no openEHR spec governs measured performance (CNF guide
//! `master03-overview.adoc` §Product Scope excludes it) — our own
//! design/extension; the journey decomposition keeps the population-anchored
//! envelope (`arrival_rate` = aggregate operation arrivals, the read:write
//! share inside the 10:1–~50:1 derivation band).

#![allow(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT; the carriers \
              here are cfg(test)-only, so #[expect] would be unfulfilled in the non-test build"
)]

use base64::Engine;
use hdrhistogram::Histogram;
use hdrhistogram::serialization::{Deserializer, Serializer, V2Serializer};
use serde::{Deserialize, Serialize};

use crate::ids::{CaseId, CorpusKey};

/// The volumetric class ladder (the §8.11 step-2c selection key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerfClass {
    /// Proof-of-concept volumes: the lowest rung of the ladder.
    #[serde(rename = "POC")]
    Poc,
    /// Small deployment volumes.
    S,
    /// Large deployment volumes.
    L,
    /// Regional/national deployment volumes: the highest rung.
    R,
}

impl PerfClass {
    /// All classes, ladder order (schema emission derives from this).
    pub const ALL: &[PerfClass] = &[PerfClass::Poc, PerfClass::S, PerfClass::L, PerfClass::R];

    /// The class's offered-load floor (peak API arrivals/s, sustained) —
    /// the published \[legislated\] defaults.
    #[must_use]
    pub fn arrival_floor_per_s(self) -> f64 {
        match self {
            PerfClass::Poc => 2.0,
            PerfClass::S => 15.0,
            PerfClass::L => 150.0,
            PerfClass::R => 1_500.0,
        }
    }

    /// The published class token (the serialized form).
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            PerfClass::Poc => "POC",
            PerfClass::S => "S",
            PerfClass::L => "L",
            PerfClass::R => "R",
        }
    }

    /// Parse a class token.
    ///
    /// # Errors
    /// The unknown token (the ladder is closed).
    pub fn parse(token: &str) -> Result<Self, String> {
        match token {
            "POC" => Ok(PerfClass::Poc),
            "S" => Ok(PerfClass::S),
            "L" => Ok(PerfClass::L),
            "R" => Ok(PerfClass::R),
            other => Err(format!("unknown performance class {other:?}")),
        }
    }
}

/// An offered rate (`15/s`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RatePerSecond(pub f64);

impl<'de> Deserialize<'de> for RatePerSecond {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let raw = s
            .strip_suffix("/s")
            .ok_or_else(|| serde::de::Error::custom(format!("rate {s:?} must end in /s")))?;
        raw.trim()
            .parse::<f64>()
            .map(Self)
            .map_err(|e| serde::de::Error::custom(format!("rate {s:?}: {e}")))
    }
}

impl Serialize for RatePerSecond {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}/s", self.0))
    }
}

/// An ISO 8601 duration in the restricted `PTnHnMnS`/`PTnM` shapes the
/// workload blocks use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkloadDuration(pub u64);

impl<'de> Deserialize<'de> for WorkloadDuration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_iso_duration_secs(&s).map(Self).ok_or_else(|| {
            serde::de::Error::custom(format!("duration {s:?} is not PT[nH][nM][nS]"))
        })
    }
}

impl Serialize for WorkloadDuration {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use std::fmt::Write as _;
        #[expect(
            clippy::integer_division,
            reason = "whole hours/minutes of an ISO 8601 duration: exact integer split"
        )]
        let (h, m, s) = (self.0 / 3600, (self.0 % 3600) / 60, self.0 % 60);
        let mut out = String::from("PT");
        if h > 0 {
            let _ = write!(out, "{h}H");
        }
        if m > 0 {
            let _ = write!(out, "{m}M");
        }
        if s > 0 || (h == 0 && m == 0) {
            let _ = write!(out, "{s}S");
        }
        serializer.serialize_str(&out)
    }
}

/// Microseconds → milliseconds (latency values are far below the f64
/// mantissa bound; the histogram's value range is capped at recording).
fn us_to_ms(us: u64) -> f64 {
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "latencies << 2^52 microseconds"
    )]
    {
        us as f64 / 1_000.0
    }
}

fn parse_iso_duration_secs(s: &str) -> Option<u64> {
    let rest = s.strip_prefix("PT")?;
    let mut total: u64 = 0;
    let mut number = String::new();
    for c in rest.chars() {
        if c.is_ascii_digit() {
            number.push(c);
        } else {
            let n: u64 = number.parse().ok()?;
            number.clear();
            total = total.checked_add(match c {
                'H' => n.checked_mul(3600)?,
                'M' => n.checked_mul(60)?,
                'S' => n,
                _ => return None,
            })?;
        }
    }
    number.is_empty().then_some(total)
}

/// A percentage share of scheduled arrivals (`61%`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Percent(pub f64);

impl<'de> Deserialize<'de> for Percent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let raw = s
            .strip_suffix('%')
            .ok_or_else(|| serde::de::Error::custom(format!("share {s:?} must end in %")))?;
        raw.trim()
            .parse::<f64>()
            .map(Self)
            .map_err(|e| serde::de::Error::custom(format!("share {s:?}: {e}")))
    }
}

impl Serialize for Percent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}%", self.0))
    }
}

/// The closed operation vocabulary a journey stage may name — each variant is
/// one concrete platform operation with a fixed ITS-REST wire realization in
/// the driver (`perf_run`).
///
/// Reads and writes are classified so the catalogue's expanded mix reconciles
/// against the derivation band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerfOp {
    /// `POST /ehr` → 201 (`I_EHR_SERVICE.create_ehr`).
    EhrCreate,
    /// `GET /ehr/{ehr_id}` → 200 (`I_EHR_SERVICE.get_ehr`).
    EhrRead,
    /// `GET /ehr/{ehr_id}/ehr_status` → 200 (`I_EHR_STATUS.get_ehr_status`).
    EhrStatusRead,
    /// `PUT /ehr/{ehr_id}/ehr_status` (If-Match) → 200/204
    /// (`I_EHR_STATUS.set_ehr_status`).
    EhrStatusUpdate,
    /// `POST /ehr/{ehr_id}/composition` → 201
    /// (`I_EHR_COMPOSITION.create_composition`).
    CompositionCommit,
    /// `GET /ehr/{ehr_id}/composition/{version_uid}` → 200
    /// (`I_EHR_COMPOSITION.get_composition_at_version`).
    CompositionRead,
    /// `GET /ehr/{ehr_id}/composition/{versioned_object_uid}` → 200
    /// (`I_EHR_COMPOSITION.get_composition_latest`).
    CompositionReadCurrent,
    /// `GET /ehr/{ehr_id}/versioned_composition/{uid}/revision_history` →
    /// 200 (`I_EHR_COMPOSITION.get_composition_revision_history`).
    CompositionRevisionHistory,
    /// `PUT /ehr/{ehr_id}/composition/{versioned_object_uid}` (If-Match) →
    /// 200/204 (`I_EHR_COMPOSITION.update_composition`).
    CompositionUpdate,
    /// `DELETE /ehr/{ehr_id}/composition/{version_uid}` → 204
    /// (`I_EHR_COMPOSITION.delete_composition`).
    CompositionDelete,
    /// `POST /ehr/{ehr_id}/directory` → 201
    /// (`I_EHR_DIRECTORY.create_directory`).
    DirectoryCreate,
    /// `GET /ehr/{ehr_id}/directory` → 200
    /// (`I_EHR_DIRECTORY.get_directory`).
    DirectoryRead,
    /// `PUT /ehr/{ehr_id}/directory` (If-Match) → 200/204
    /// (`I_EHR_DIRECTORY.update_directory`).
    DirectoryUpdate,
    /// `POST /ehr/{ehr_id}/contribution` → 201
    /// (`I_EHR_CONTRIBUTION.commit_contribution`).
    ContributionCommit,
    /// `GET /ehr/{ehr_id}/contribution/{uid}` → 200
    /// (`I_EHR_CONTRIBUTION.get_contribution`).
    ContributionRead,
    /// `POST /query/aql` (EHR-scoped) → 200
    /// (`I_QUERY_SERVICE.execute_ad_hoc_query`).
    AdhocQuery,
    /// `POST /query/aql` (cross-EHR ward worklist) → 200
    /// (`I_QUERY_SERVICE.execute_ad_hoc_query`).
    WardQuery,
    /// `GET /query/{name}/{version}` → 200
    /// (`I_QUERY_SERVICE.execute_stored_query`).
    StoredQueryExecute,
    /// `GET /definition/template/adl1.4` → 200
    /// (`I_DEFINITION_ADL14.list_templates`).
    TemplateList,
    /// `GET /definition/template/adl1.4/{template_id}` → 200
    /// (`I_DEFINITION_ADL14.get_template`).
    TemplateGet,
    /// `PUT /ehr/{ehr_id}/composition/{uid}/tags` → 200 (`ITEM_TAG` update;
    /// ITS-REST TAGS API).
    TagsPut,
    /// `GET /ehr/{ehr_id}/composition/{uid}/tags` → 200 (`ITEM_TAG` read;
    /// ITS-REST TAGS API).
    TagsRead,
    /// `GET /ehr/{ehr_id}/versioned_composition/{vo_uid}/version/{version_uid}`
    /// → 200 (`I_EHR_COMPOSITION.get_composition_at_version`, the
    /// `ORIGINAL_VERSION` envelope — the version-signature read side).
    CompositionVersionRead,
    /// `POST /ehr/{ehr_id}/composition` in the Simplified FLAT form
    /// (`application/openehr.wt.flat+json` + `openehr-template-id`) → 201
    /// (`I_EHR_COMPOSITION.create_composition`).
    CompositionCommitFlat,
    /// `GET /ehr/{ehr_id}/composition/{version_uid}` with the Simplified
    /// FLAT `Accept` → 200
    /// (`I_EHR_COMPOSITION.get_composition_at_version`).
    CompositionReadFlat,
    /// `POST /demographic/person` → 201
    /// (`I_DEMOGRAPHIC_SERVICE.create_party`).
    PartyCreate,
    /// `GET /demographic/person/{versioned_object_uid}` → 200
    /// (`I_PARTY.get_party`).
    PartyRead,
    /// `PUT /demographic/person/{versioned_object_uid}` (If-Match) → 200/204
    /// (`I_PARTY.update_party`).
    PartyUpdate,
    /// `POST /demographic/party_relationship` → 201
    /// (`I_DEMOGRAPHIC_SERVICE.create_party_relationship`) — an EXTENSION
    /// route: ITS-REST 1.1.0 surfaces no `PARTY_RELATIONSHIP` resource, so
    /// no openEHR spec governs it (our own design/extension, register
    /// AMB-32, declared in `vocab/wire_surface.yaml`).
    PartyRelationshipCreate,
    /// `GET /demographic/party_relationship/{versioned_object_uid}` → 200
    /// (`I_PARTY_RELATIONSHIP.get_party_relationship`) — the same
    /// extension route family as [`PerfOp::PartyRelationshipCreate`].
    PartyRelationshipRead,
    /// `GET /definition/template/adl1.4/{template_id}/example` → 200
    /// (`I_DEFINITION_ADL14.get_opt`, example variant).
    TemplateExample,
    /// `GET /definition/template/adl2` → 200
    /// (`I_DEFINITION_ADL2.list_templates`).
    TemplateAdl2List,
    /// `GET /definition/archetype/adl2` → 200
    /// (`I_DEFINITION_ADL2.list_archetypes`) — an EXTENSION route: ITS-REST
    /// 1.1.0 surfaces no ADL 2 archetype resource, so no openEHR spec governs
    /// it (our own design/extension, register AMB-37, declared in
    /// `vocab/wire_surface.yaml`).
    ArchetypeAdl2List,
    /// `GET /admin/report/contribution/count?a_service=Ehr` → 200
    /// (`I_ADMIN_SERVICE.contribution_count`) — an EXTENSION route: the
    /// released Admin API is the two EHR deletes alone, so no openEHR spec
    /// governs it (our own design/extension, register AMB-33, declared in
    /// `vocab/wire_surface.yaml`).
    AdminContributionReport,
    /// `GET /message/export/{ehr_id}` → 200
    /// (`I_EHR_EXTRACT_SERVICE.export_ehrs`) — an EXTENSION route: ITS-REST
    /// 1.1.0 publishes no MESSAGE API at all, so no openEHR spec governs it
    /// (our own design/extension, register AMB-34, declared in
    /// `vocab/wire_surface.yaml`).
    EhrExtractExport,
    /// `POST /message/tdd/{ehr_id}` → 201 (`I_TDD_SERVICE.import_tdd`) — an
    /// EXTENSION route on the same register entry. A WRITE: the converted
    /// COMPOSITION commits through the ordinary validated path, so a TDD
    /// import is a document-authoring arrival in another input serialization,
    /// exactly as [`PerfOp::CompositionCommitFlat`] is.
    TddImport,
    /// `POST /query/aql` with an ORDER BY + LIMIT projection → 200
    /// (`I_QUERY_SERVICE.execute_ad_hoc_query`, the advanced-AQL class).
    AnalyticsQuery,
    /// `POST /query/aql` with a `TERMINOLOGY('expand', …)` predicate → 200
    /// (`I_QUERY_SERVICE.execute_ad_hoc_query`, the terminology-backed
    /// class).
    TerminologyQuery,
    /// `OPTIONS /` → 200 + `Allow` (the System API's one operation).
    SystemOptions,
    /// `GET /.well-known/smart-configuration` on the PLATFORM base → 200
    /// (the SMART on openEHR service-discovery document).
    SmartConfigurationRead,
    /// `GET /ehr/{ehr_id}` presented with NO credentials → 401 (the
    /// authenticated-access boundary under sustained load).
    UnauthenticatedProbe,
    /// `POST /ehr/{ehr_id}/composition` presented by the read-only
    /// principal → 403 (the authorization boundary under sustained load;
    /// the request mutates nothing, so it is not a write arrival).
    ReadonlyWriteDenied,
}

/// Which ixit-declared principal a measured arrival is driven by.
///
/// The primary is the party's default `sut` instance; the others are named
/// instances a party MAY declare — a party that declares none simply does not
/// run the journeys that need them (an undeclared fact costs coverage, never
/// correctness), so the ixit stays the single source of deployment facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Principal {
    /// The party's default `sut` instance.
    Primary,
    /// The ixit `unauthenticated` instance (no credentials at all).
    Unauthenticated,
    /// The ixit `readonly` instance (a principal without write grants).
    ReadOnly,
    /// The instance the ixit's `smart.platform_instance` names — the SMART
    /// *Platform* base URL, a different path root from the openEHR REST
    /// base (ITS-REST `docs/smart_app_launch/master04-service_discovery.adoc`
    /// §Service Discovery).
    SmartPlatform,
    /// The ixit `admin` instance — the ADMIN-role principal every
    /// admin-gated stage rides (the same instance the functional admin
    /// cases address with `on: admin`). A party that declares no admin
    /// instance drops the journeys that need it, exactly like the other
    /// boundary principals.
    Admin,
}

impl PerfOp {
    /// All operations, vocabulary order (schema emission + the coverage
    /// report derive from this).
    pub const ALL: &[PerfOp] = &[
        PerfOp::EhrCreate,
        PerfOp::EhrRead,
        PerfOp::EhrStatusRead,
        PerfOp::EhrStatusUpdate,
        PerfOp::CompositionCommit,
        PerfOp::CompositionRead,
        PerfOp::CompositionReadCurrent,
        PerfOp::CompositionRevisionHistory,
        PerfOp::CompositionUpdate,
        PerfOp::CompositionDelete,
        PerfOp::DirectoryCreate,
        PerfOp::DirectoryRead,
        PerfOp::DirectoryUpdate,
        PerfOp::ContributionCommit,
        PerfOp::ContributionRead,
        PerfOp::AdhocQuery,
        PerfOp::WardQuery,
        PerfOp::StoredQueryExecute,
        PerfOp::TemplateList,
        PerfOp::TemplateGet,
        PerfOp::TagsPut,
        PerfOp::TagsRead,
        PerfOp::CompositionVersionRead,
        PerfOp::CompositionCommitFlat,
        PerfOp::CompositionReadFlat,
        PerfOp::PartyCreate,
        PerfOp::PartyRead,
        PerfOp::PartyUpdate,
        PerfOp::PartyRelationshipCreate,
        PerfOp::PartyRelationshipRead,
        PerfOp::TemplateExample,
        PerfOp::TemplateAdl2List,
        PerfOp::ArchetypeAdl2List,
        PerfOp::AdminContributionReport,
        PerfOp::EhrExtractExport,
        PerfOp::TddImport,
        PerfOp::AnalyticsQuery,
        PerfOp::TerminologyQuery,
        PerfOp::SystemOptions,
        PerfOp::SmartConfigurationRead,
        PerfOp::UnauthenticatedProbe,
        PerfOp::ReadonlyWriteDenied,
    ];

    /// Parse a journey-stage operation name.
    ///
    /// # Errors
    /// The unknown name (the operation vocabulary is closed).
    pub fn parse(name: &str) -> Result<Self, String> {
        PerfOp::ALL
            .iter()
            .copied()
            .find(|op| op.as_str() == name)
            .ok_or_else(|| format!("unknown journey operation {name:?}"))
    }

    /// The vocabulary name (journey stages, measurement labels).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PerfOp::EhrCreate => "ehr_create",
            PerfOp::EhrRead => "ehr_read",
            PerfOp::EhrStatusRead => "ehr_status_read",
            PerfOp::EhrStatusUpdate => "ehr_status_update",
            PerfOp::CompositionCommit => "composition_commit",
            PerfOp::CompositionRead => "composition_read",
            PerfOp::CompositionReadCurrent => "composition_read_current",
            PerfOp::CompositionRevisionHistory => "composition_revision_history",
            PerfOp::CompositionUpdate => "composition_update",
            PerfOp::CompositionDelete => "composition_delete",
            PerfOp::DirectoryCreate => "directory_create",
            PerfOp::DirectoryRead => "directory_read",
            PerfOp::DirectoryUpdate => "directory_update",
            PerfOp::ContributionCommit => "contribution_commit",
            PerfOp::ContributionRead => "contribution_read",
            PerfOp::AdhocQuery => "adhoc_query",
            PerfOp::WardQuery => "ward_query",
            PerfOp::StoredQueryExecute => "stored_query_execute",
            PerfOp::TemplateList => "template_list",
            PerfOp::TemplateGet => "template_get",
            PerfOp::TagsPut => "tags_put",
            PerfOp::TagsRead => "tags_read",
            PerfOp::CompositionVersionRead => "composition_version_read",
            PerfOp::CompositionCommitFlat => "composition_commit_flat",
            PerfOp::CompositionReadFlat => "composition_read_flat",
            PerfOp::PartyCreate => "party_create",
            PerfOp::PartyRead => "party_read",
            PerfOp::PartyUpdate => "party_update",
            PerfOp::PartyRelationshipCreate => "party_relationship_create",
            PerfOp::PartyRelationshipRead => "party_relationship_read",
            PerfOp::TemplateExample => "template_example",
            PerfOp::TemplateAdl2List => "template_adl2_list",
            PerfOp::ArchetypeAdl2List => "archetype_adl2_list",
            PerfOp::AdminContributionReport => "admin_contribution_report",
            PerfOp::EhrExtractExport => "ehr_extract_export",
            PerfOp::TddImport => "tdd_import",
            PerfOp::AnalyticsQuery => "analytics_query",
            PerfOp::TerminologyQuery => "terminology_query",
            PerfOp::SystemOptions => "system_options",
            PerfOp::SmartConfigurationRead => "smart_configuration_read",
            PerfOp::UnauthenticatedProbe => "unauthenticated_probe",
            PerfOp::ReadonlyWriteDenied => "readonly_write_denied",
        }
    }

    /// The ixit principal the arrival is driven by (see [`Principal`]).
    #[must_use]
    pub fn principal(self) -> Principal {
        match self {
            PerfOp::UnauthenticatedProbe => Principal::Unauthenticated,
            PerfOp::ReadonlyWriteDenied => Principal::ReadOnly,
            PerfOp::SmartConfigurationRead => Principal::SmartPlatform,
            // The activity report is admin-gated on the served extension
            // route (the 2026-07-29 POC window drove it with the primary
            // principal and 403'd on every arrival).
            PerfOp::AdminContributionReport => Principal::Admin,
            _ => Principal::Primary,
        }
    }

    /// Whether the operation mutates platform state (the reconciliation
    /// class: the expanded write share must stay inside the derivation
    /// band). A DENIED write ([`PerfOp::ReadonlyWriteDenied`]) mutates
    /// nothing, so it is not a write arrival — the band reconciles the
    /// mutation mix, not the request method.
    #[must_use]
    pub fn is_write(self) -> bool {
        matches!(
            self,
            PerfOp::EhrCreate
                | PerfOp::EhrStatusUpdate
                | PerfOp::CompositionCommit
                | PerfOp::CompositionCommitFlat
                | PerfOp::CompositionUpdate
                | PerfOp::CompositionDelete
                | PerfOp::DirectoryCreate
                | PerfOp::DirectoryUpdate
                | PerfOp::ContributionCommit
                | PerfOp::TagsPut
                | PerfOp::PartyCreate
                | PerfOp::PartyUpdate
                | PerfOp::PartyRelationshipCreate
                | PerfOp::TddImport
        )
    }

    /// Whether a journey stage of this operation must name a `template`
    /// (the payload's constraint carrier).
    #[must_use]
    pub fn needs_template(self) -> bool {
        matches!(
            self,
            PerfOp::CompositionCommit
                | PerfOp::CompositionUpdate
                | PerfOp::ContributionCommit
                | PerfOp::ReadonlyWriteDenied
        )
    }

    /// Whether the operation's prerequisite lives ONLY in its journey
    /// instance's own captured state, with no standing-ward fallback. An
    /// instance whose earlier stages fell before the measured window could
    /// never resolve one, so such a journey is scheduled only when the whole
    /// instance fits inside the window — the alternative would be error
    /// arrivals manufactured by the instrument rather than observed from the
    /// SUT.
    #[must_use]
    pub fn needs_instance_prerequisite(self) -> bool {
        matches!(
            self,
            // The deletion journey deletes ITS OWN commit, never a shared
            // ward document.
            PerfOp::CompositionDelete
                // The demographic chain hangs off the party this instance
                // registered; the standing ward has no seeded party.
                | PerfOp::PartyRead
                | PerfOp::PartyUpdate
                | PerfOp::PartyRelationshipCreate
                | PerfOp::PartyRelationshipRead
        )
    }

    /// Which auxiliary committed payload (beyond the CKM template pack) the
    /// operation's body comes from — `None` for every operation that
    /// carries no body or builds it from a pack template.
    #[must_use]
    pub fn aux_payload(self) -> Option<AuxPayloadKind> {
        match self {
            PerfOp::CompositionCommitFlat => Some(AuxPayloadKind::Flat),
            PerfOp::PartyCreate | PerfOp::PartyUpdate => Some(AuxPayloadKind::Person),
            PerfOp::PartyRelationshipCreate => Some(AuxPayloadKind::PartyRelationship),
            PerfOp::TddImport => Some(AuxPayloadKind::Tdd),
            _ => None,
        }
    }

    /// The claimed capabilities (capability-matrix keys) one measured
    /// arrival of this operation exercises — the certificate's workload
    /// coverage joins this against the ICS claims: a claimed capability no
    /// journey touches is a catalogue gap, listed explicitly.
    #[must_use]
    pub fn capabilities(self) -> &'static [&'static str] {
        match self {
            // A created EHR carries the default EHR_STATUS, whose subject is
            // a PARTY_SELF with no external ref — the anonymous form (RM ehr
            // §EHR Status), which is exactly what the AnonymousEhrs
            // capability names.
            PerfOp::EhrCreate => &["EhrOperations", "AnonymousEhrs", "EhrApi"],
            PerfOp::EhrRead => &["EhrOperations", "EhrApi"],
            // Reading that status back is the surface on which the clinical
            // record is shown to expose no demographic identity.
            PerfOp::EhrStatusRead => &["EhrStatus", "EhrDemographicSeparation", "EhrApi"],
            PerfOp::EhrStatusUpdate => &["EhrStatus", "Versioning", "EhrApi"],
            // Every commit/update is validated against its template on the
            // way in (the walker over the WebTemplate + RM invariants).
            PerfOp::CompositionCommit => &["CompositionOps", "ArchetypeValidation", "EhrApi"],
            PerfOp::CompositionRead | PerfOp::CompositionReadCurrent => {
                &["CompositionOps", "EhrApi"]
            }
            PerfOp::CompositionRevisionHistory | PerfOp::CompositionDelete => {
                &["CompositionOps", "Versioning", "EhrApi"]
            }
            PerfOp::CompositionUpdate => &[
                "CompositionOps",
                "Versioning",
                "ArchetypeValidation",
                "EhrApi",
            ],
            PerfOp::DirectoryCreate | PerfOp::DirectoryRead | PerfOp::DirectoryUpdate => {
                &["DirectoryOps", "EhrApi"]
            }
            // Every commit rides a CONTRIBUTION carrying the server-set
            // commit AUDIT_DETAILS (RM common §change_control), and reading
            // it back is the accountability trail's read side.
            PerfOp::ContributionCommit => &[
                "ChangeSets",
                "ArchetypeValidation",
                "Versioning",
                "AuditAccountability",
                "EhrApi",
            ],
            PerfOp::ContributionRead => &["ChangeSets", "AuditAccountability", "EhrApi"],
            PerfOp::AdhocQuery | PerfOp::WardQuery => &["AqlBasic", "QueryApi"],
            PerfOp::StoredQueryExecute => &["QueryProvisioning", "AqlBasic", "QueryApi"],
            PerfOp::TemplateList | PerfOp::TemplateGet => {
                &["Adl14OptProvisioning", "DefinitionApi"]
            }
            PerfOp::TemplateExample => {
                &["TemplateExamples", "Adl14OptProvisioning", "DefinitionApi"]
            }
            PerfOp::TemplateAdl2List => &["Adl2OptProvisioning", "DefinitionApi"],
            // Extension routes (no released wire) — like the
            // PARTY_RELATIONSHIP pair above they gate their own CAPABILITY
            // only, so the released-wire API capabilities (DefinitionApi,
            // AdminApi) are deliberately absent here.
            PerfOp::ArchetypeAdl2List => &["Adl2ArchetypeProvisioning"],
            PerfOp::AdminContributionReport => &["ActivityReport"],
            // The whole-EHR extract read is the ONE operation the CNF
            // Profiles book's MESSAGE API row can rest on here: the row
            // names an API the release does not publish, so what the
            // arrival exercises is this product's own message surface.
            PerfOp::EhrExtractExport => &["EhrExtract", "MessageApi"],
            PerfOp::TddImport => &["Tds", "ArchetypeValidation"],
            // ITEM_TAG rides the EHR API's tag resources, but it is its own
            // capability: a service that answers every EHR-API operation and
            // no tag route is still conformant (ITS-REST overview
            // §openehr-item-tag: "If the server does not support ITEM_TAGs,
            // these headers will also be unsupported").
            PerfOp::TagsPut | PerfOp::TagsRead => &["ItemTags", "EhrApi"],
            // The ORIGINAL_VERSION envelope is where the version signature
            // is carried (RM common §change_control, Digital Signature).
            PerfOp::CompositionVersionRead => {
                &["Signing", "Versioning", "CompositionOps", "EhrApi"]
            }
            PerfOp::CompositionCommitFlat => &[
                "SimplifiedFormats",
                "CompositionOps",
                "ArchetypeValidation",
                "EhrApi",
            ],
            PerfOp::CompositionReadFlat => &["SimplifiedFormats", "CompositionOps", "EhrApi"],
            // Every party commit is validated against the archetyped
            // demographic model on the way in — the same reasoning that maps
            // ArchetypeValidation onto CompositionCommit.
            PerfOp::PartyCreate | PerfOp::PartyUpdate => &[
                "PartyOperations",
                "DemographicArchetypeValidation",
                "DemographicApi",
            ],
            PerfOp::PartyRead => &["PartyOperations", "DemographicApi"],
            // Extension routes (no released wire) — they gate the
            // PartyRelationshipOperations CAPABILITY only, never the
            // DEMOGRAPHIC API's wire conformance, so DemographicApi is
            // deliberately absent here.
            PerfOp::PartyRelationshipCreate | PerfOp::PartyRelationshipRead => {
                &["PartyRelationshipOperations"]
            }
            PerfOp::AnalyticsQuery => &["AqlAdvanced", "AqlBasic", "QueryApi"],
            PerfOp::TerminologyQuery => &["AqlTerminology", "AqlBasic", "QueryApi"],
            PerfOp::SystemOptions => &["SystemApi"],
            PerfOp::SmartConfigurationRead => &["SmartAppLaunch"],
            PerfOp::UnauthenticatedProbe => &["AuthenticatedAccess"],
            PerfOp::ReadonlyWriteDenied => &["AuthorizationSeparation"],
        }
    }
}

/// The auxiliary committed payloads of the non-COMPOSITION journey stages.
///
/// Each one is a corpus fixture the functional
/// catalogue already adjudicates, never a payload invented for the load
/// instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuxPayloadKind {
    /// The Simplified FLAT composition + the OPT it is constrained by.
    Flat,
    /// The demographic `PERSON` (create + amended update state).
    Person,
    /// The `PARTY_RELATIONSHIP` the extension route commits.
    PartyRelationship,
    /// The Template Data Document the TDD-import extension route commits,
    /// plus the operational template it instantiates.
    Tdd,
}

/// A journey stage's planned offset from the journey's arrival instant.
///
/// Every form is deterministic under the seeded schedule (uniform draws hash
/// the journey/stage indices — two runners produce the same instants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOffset {
    /// Exactly `s` seconds after the journey arrival.
    Fixed(u64),
    /// Uniformly hashed into `[min_s, max_s]` (e.g. a laboratory
    /// turnaround band).
    Uniform {
        /// Inclusive lower bound of the draw, seconds after arrival.
        min_s: u64,
        /// Inclusive upper bound of the draw, seconds after arrival.
        max_s: u64,
    },
    /// `count` repetitions at `k * interval_s` (the medication round).
    Periodic {
        /// Seconds between consecutive repetitions.
        interval_s: u64,
        /// How many repetitions the stage expands into.
        count: u32,
    },
}

impl StageOffset {
    /// How many operation arrivals the stage expands into.
    #[must_use]
    pub fn arrivals(self) -> u64 {
        match self {
            StageOffset::Fixed(_) | StageOffset::Uniform { .. } => 1,
            StageOffset::Periodic { count, .. } => u64::from(count),
        }
    }

    /// The latest possible offset (seconds) — the journey's span bound.
    #[must_use]
    pub fn max_offset_s(self) -> u64 {
        match self {
            StageOffset::Fixed(s) => s,
            StageOffset::Uniform { max_s, .. } => max_s,
            StageOffset::Periodic { interval_s, count } => {
                interval_s.saturating_mul(u64::from(count.saturating_sub(1)))
            }
        }
    }
}

impl<'de> Deserialize<'de> for StageOffset {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PeriodicSpec {
            interval: WorkloadDuration,
            count: u32,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        enum Structured {
            #[serde(rename = "uniform")]
            Uniform([WorkloadDuration; 2]),
            #[serde(rename = "periodic")]
            Periodic(PeriodicSpec),
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Fixed(WorkloadDuration),
            Structured(Structured),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Fixed(d) => Ok(StageOffset::Fixed(d.0)),
            Raw::Structured(Structured::Uniform([min, max])) => {
                if min.0 > max.0 {
                    return Err(serde::de::Error::custom(format!(
                        "uniform offset [{}, {}] is inverted",
                        min.0, max.0
                    )));
                }
                Ok(StageOffset::Uniform {
                    min_s: min.0,
                    max_s: max.0,
                })
            }
            Raw::Structured(Structured::Periodic(p)) => {
                if p.count == 0 {
                    return Err(serde::de::Error::custom("periodic count must be >= 1"));
                }
                Ok(StageOffset::Periodic {
                    interval_s: p.interval.0,
                    count: p.count,
                })
            }
        }
    }
}

impl Serialize for StageOffset {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match *self {
            StageOffset::Fixed(s) => WorkloadDuration(s).serialize(serializer),
            StageOffset::Uniform { min_s, max_s } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry(
                    "uniform",
                    &[WorkloadDuration(min_s), WorkloadDuration(max_s)],
                )?;
                map.end()
            }
            StageOffset::Periodic { interval_s, count } => {
                #[derive(Serialize)]
                struct PeriodicSpec {
                    interval: WorkloadDuration,
                    count: u32,
                }
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry(
                    "periodic",
                    &PeriodicSpec {
                        interval: WorkloadDuration(interval_s),
                        count,
                    },
                )?;
                map.end()
            }
        }
    }
}

/// One ordered stage of a clinical journey: a platform operation at a
/// planned offset, optionally carrying its payload template.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JourneyStage {
    /// A [`PerfOp`] vocabulary name.
    pub op: String,
    /// The corpus template key the stage commits/updates against (required
    /// exactly when [`PerfOp::needs_template`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// When the stage's operation arrives, relative to the journey arrival.
    pub at: StageOffset,
}

/// One clinical journey: an ordered, time-offset operation sequence with
/// its activity-statistics ground.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Journey {
    /// What clinical journey this models, in one phrase.
    pub description: String,
    /// The official activity statistic the journey's shape/rate traces to
    /// (the same register the class floors derive from).
    pub derivation: String,
    /// The journey's operations, in arrival order.
    pub stages: Vec<JourneyStage>,
}

impl Journey {
    /// Operation arrivals one instance expands into.
    #[must_use]
    pub fn arrivals(&self) -> u64 {
        self.stages.iter().map(|s| s.at.arrivals()).sum()
    }

    /// The journey's span bound (latest stage offset, seconds).
    #[must_use]
    pub fn max_offset_s(&self) -> u64 {
        self.stages
            .iter()
            .map(|s| s.at.max_offset_s())
            .max()
            .unwrap_or(0)
    }
}

/// The journey catalogue — the hospital-simulation vocabulary
/// (`vocab/journey_catalogue.yaml`): every journey a workload may name,
/// each stage a closed-vocabulary operation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JourneyCatalogue(
    #[serde(deserialize_with = "crate::model::de::ordered_map")] pub Vec<(String, Journey)>,
);

/// The derivation band the expanded operation mix must reconcile with.
///
/// Write share within \[100/(50+1), 100/(10+1)\] percent — the 10:1
/// read-heavy OLTP convention (YCSB/OLTP-Bench) as the floor's mix, ~50:1
/// as the audit-log-evidenced read-heavy ceiling.
pub const WRITE_SHARE_BAND: (f64, f64) = (100.0 / 51.0, 100.0 / 11.0);

/// A workload's expansion through the catalogue: per-operation shares of
/// scheduled arrivals, the mean arrivals per journey, and the write share.
#[derive(Debug, Clone)]
pub struct Expansion {
    /// Mean operation arrivals per journey instance (> 0).
    pub arrivals_per_journey: f64,
    /// Share of scheduled operation arrivals per operation (sums to 100).
    pub op_shares: Vec<(PerfOp, f64)>,
    /// The expanded write share (percent of operation arrivals).
    pub write_share: f64,
}

impl JourneyCatalogue {
    /// Look up a journey by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Journey> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, journey)| journey)
    }

    /// Catalogue shape invariants: non-empty journeys, known operations,
    /// template presence exactly where the operation needs one.
    ///
    /// # Errors
    /// The violated invariant, naming the journey/stage.
    pub fn check_invariants(&self) -> Result<(), String> {
        if self.0.is_empty() {
            return Err("journey catalogue is empty".to_owned());
        }
        for (name, journey) in &self.0 {
            if journey.stages.is_empty() {
                return Err(format!("journey {name} has no stages"));
            }
            for (i, stage) in journey.stages.iter().enumerate() {
                let op = PerfOp::parse(&stage.op)
                    .map_err(|e| format!("journey {name} stage {i}: {e}"))?;
                if op.needs_template() && stage.template.is_none() {
                    return Err(format!(
                        "journey {name} stage {i} ({}) needs a template",
                        stage.op
                    ));
                }
                if !op.needs_template() && stage.template.is_some() {
                    return Err(format!(
                        "journey {name} stage {i} ({}) must not carry a template",
                        stage.op
                    ));
                }
            }
        }
        Ok(())
    }

    /// Expand journey shares into the per-operation arrival mix and check
    /// the population-anchored envelope: shares sum to 100, every share
    /// names a catalogue journey, and the expanded write share lies inside
    /// [`WRITE_SHARE_BAND`].
    ///
    /// # Errors
    /// The violated reconciliation rule.
    pub fn expansion(&self, shares: &[(String, Percent)]) -> Result<Expansion, String> {
        if shares.is_empty() {
            return Err("workload journeys is empty".to_owned());
        }
        let sum: f64 = shares.iter().map(|(_, p)| p.0).sum();
        if (sum - 100.0).abs() >= 0.01 {
            return Err(format!("journey shares sum to {sum}%, must be 100%"));
        }
        let mut op_weight: Vec<(PerfOp, f64)> = Vec::new();
        let mut arrivals_per_journey = 0.0;
        for (name, share) in shares {
            let journey = self
                .get(name)
                .ok_or_else(|| format!("workload names unknown journey {name:?}"))?;
            for stage in &journey.stages {
                let op = PerfOp::parse(&stage.op)?;
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_precision_loss,
                    reason = "stage arrival counts are tiny"
                )]
                let weight = share.0 / 100.0 * stage.at.arrivals() as f64;
                arrivals_per_journey += weight;
                if let Some((_, w)) = op_weight.iter_mut().find(|(o, _)| *o == op) {
                    *w += weight;
                } else {
                    op_weight.push((op, weight));
                }
            }
        }
        if arrivals_per_journey <= 0.0 {
            return Err("journey expansion yields zero arrivals".to_owned());
        }
        let op_shares: Vec<(PerfOp, f64)> = op_weight
            .iter()
            .map(|(op, w)| (*op, w / arrivals_per_journey * 100.0))
            .collect();
        let write_share: f64 = op_shares
            .iter()
            .filter(|(op, _)| op.is_write())
            .map(|(_, s)| s)
            .sum();
        let (min_w, max_w) = WRITE_SHARE_BAND;
        if !(min_w..=max_w).contains(&write_share) {
            return Err(format!(
                "expanded write share {write_share:.2}% is outside the derivation band \
                 [{min_w:.2}%, {max_w:.2}%] (10:1..50:1 read:write)"
            ));
        }
        Ok(Expansion {
            arrivals_per_journey,
            op_shares,
            write_share,
        })
    }
}

/// The arrival-time shape over the measured window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalCurve {
    /// Evenly spaced arrivals (the normative hour's shape).
    #[default]
    Uniform,
    /// The hospital day curve — busy-hour peaking per the ITU-T E.500
    /// busy-hour convention the floors' derivation already cites; valid
    /// only for the extended (>= 8 h) holds. The busy-hour buckets meet
    /// the class floor; the whole-window mean sits below it.
    Diurnal,
}

/// The OPEN-LOOP offered load: a seeded arrival schedule of clinical
/// journeys. `arrival_rate` stays aggregate OPERATION arrivals/s (the
/// class floor's unit); the journey shares decompose it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Workload {
    /// Aggregate operation arrivals per second the schedule offers.
    pub arrival_rate: RatePerSecond,
    /// The unrecorded warmup preceding the measured window.
    pub warmup: WorkloadDuration,
    /// The recorded measurement window.
    pub duration: WorkloadDuration,
    /// The arrival-time shape (default uniform; diurnal only for the
    /// extended holds).
    #[serde(default)]
    pub arrival_curve: ArrivalCurve,
    /// journeys = share of scheduled JOURNEY instances per catalogue
    /// journey (the inner decomposition of the population-anchored
    /// envelope).
    #[serde(deserialize_with = "crate::model::de::ordered_map")]
    pub journeys: Vec<(String, Percent)>,
}

impl Workload {
    /// The journey shares must sum to 100% (±0.01).
    ///
    /// # Errors
    /// Returns the actual sum on violation.
    pub fn check_journeys(&self) -> Result<(), String> {
        let sum: f64 = self.journeys.iter().map(|(_, p)| p.0).sum();
        if (sum - 100.0).abs() < 0.01 {
            Ok(())
        } else {
            Err(format!("workload journeys sum to {sum}%, must be 100%"))
        }
    }
}

/// One threshold (ALL must hold in the single measured run).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Threshold {
    /// The measured quantity this threshold bounds.
    pub metric: Metric,
    /// The operation the metric is scoped to (absent = run-wide).
    #[serde(default)]
    pub operation: Option<String>,
    /// Upper bound (latencies: milliseconds; `error_rate`: fraction).
    #[serde(default)]
    pub max: Option<f64>,
    /// Lower bound (`offered_load_sustained`: arrivals/s).
    #[serde(default)]
    pub min: Option<f64>,
}

/// The closed threshold-metric vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Median request latency.
    LatencyP50,
    /// 90th-percentile request latency.
    LatencyP90,
    /// 99th-percentile request latency.
    LatencyP99,
    /// Failed requests as a fraction of all requests.
    ErrorRate,
    /// The arrival rate the run actually sustained, arrivals/s.
    OfferedLoadSustained,
}

/// A `kind: performance` case (its own schema family; carries `class`
/// instead of `capabilities`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceCase {
    /// The globally unique case id.
    pub id: CaseId,
    /// Always the literal `performance`.
    pub kind: String,
    /// The schedule chapter the case belongs to (always `PERFORMANCE`).
    pub component: String,
    /// The schedule's Description row.
    pub description: String,
    /// The ISO/IEC 9646 test purpose — one narrow conformance requirement.
    pub test_purpose: String,
    /// Citations (component + document + section); link-checked.
    pub spec_refs: Vec<String>,
    /// The selection key (§8.11 step 2c) — the claimed class selects.
    pub class: PerfClass,
    /// The seeded corpus the run measures against.
    pub corpus: CorpusKey,
    /// The offered load the run drives.
    pub workload: Workload,
    /// Every bound that must hold for the class to be earned.
    pub thresholds: Vec<Threshold>,
}

impl PerformanceCase {
    /// Shape invariants: kind literal, mix sums, thresholds carry a bound,
    /// and the offered-load floor is consistent with the class table.
    ///
    /// # Errors
    /// Returns the violated invariant.
    pub fn check_invariants(&self) -> Result<(), String> {
        if self.kind != "performance" {
            return Err(format!("kind must be `performance`, got {:?}", self.kind));
        }
        if self.component != "PERFORMANCE" {
            return Err(format!(
                "component must be PERFORMANCE, got {:?}",
                self.component
            ));
        }
        self.workload.check_journeys()?;
        for t in &self.thresholds {
            if t.max.is_none() && t.min.is_none() {
                return Err("threshold carries neither max nor min".to_owned());
            }
        }
        let floor = self.class.arrival_floor_per_s();
        if self.workload.arrival_rate.0 < floor {
            return Err(format!(
                "workload arrival_rate {}/s is below the class floor {floor}/s",
                self.workload.arrival_rate.0
            ));
        }
        if self.workload.arrival_curve == ArrivalCurve::Diurnal
            && self.workload.duration.0 < 8 * 3600
        {
            return Err(
                "diurnal arrival curve requires an extended hold (duration >= PT8H)".to_owned(),
            );
        }
        Ok(())
    }
}

/// One operation's measurement record — thresholds re-checkable from the
/// artifact: the histogram is the standard V2 encoding, base64-wrapped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationMeasurement {
    /// The operation these numbers are scoped to.
    pub operation: String,
    /// Requests recorded in the measured window.
    pub requests: u64,
    /// How many of those requests failed.
    pub errors: u64,
    /// Median latency, milliseconds (re-derivable from the histogram).
    pub latency_ms_p50: f64,
    /// 90th-percentile latency, milliseconds.
    pub latency_ms_p90: f64,
    /// 99th-percentile latency, milliseconds.
    pub latency_ms_p99: f64,
    /// Standard `HdrHistogram` V2 encoding, base64 (values in microseconds).
    pub hdr_v2_base64: String,
}

impl OperationMeasurement {
    /// Build the record from a recorded histogram (values in microseconds).
    ///
    /// # Errors
    /// Returns a message on serialization failure.
    pub fn from_histogram(
        operation: &str,
        histogram: &Histogram<u64>,
        errors: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            operation: operation.to_owned(),
            requests: histogram.len(),
            errors,
            latency_ms_p50: us_to_ms(histogram.value_at_quantile(0.50)),
            latency_ms_p90: us_to_ms(histogram.value_at_quantile(0.90)),
            latency_ms_p99: us_to_ms(histogram.value_at_quantile(0.99)),
            hdr_v2_base64: encode_hdr_v2(histogram)?,
        })
    }

    /// Decode the embedded histogram (the RE-CHECK path: any consumer can
    /// recompute every percentile from the artifact alone).
    ///
    /// # Errors
    /// Returns a message on a corrupt encoding.
    pub fn decode_histogram(&self) -> Result<Histogram<u64>, String> {
        decode_hdr_v2(&self.hdr_v2_base64)
    }
}

/// Encodes a recorded histogram in the standard `HdrHistogram` V2
/// serialization, base64-wrapped.
///
/// Every instrument that embeds a histogram in an artifact goes through here,
/// so one encoding reaches every artifact family.
///
/// # Errors
/// Returns a message on serialization failure.
pub fn encode_hdr_v2(histogram: &Histogram<u64>) -> Result<String, String> {
    let mut buffer = Vec::new();
    V2Serializer::new()
        .serialize(histogram, &mut buffer)
        .map_err(|e| format!("hdr serialize: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&buffer))
}

/// Decodes a base64-wrapped `HdrHistogram` V2 serialization.
///
/// # Errors
/// Returns a message on a corrupt encoding.
pub fn decode_hdr_v2(encoded: &str) -> Result<Histogram<u64>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("hdr base64: {e}"))?;
    Deserializer::new()
        .deserialize(&mut bytes.as_slice())
        .map_err(|e| format!("hdr decode: {e}"))
}

/// The container roles the resource sampler distinguishes (closed
/// vocabulary): the SUT process and its database — the split shows where a
/// class's headroom actually burns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerRole {
    /// The system under test's own server process.
    Sut,
    /// The database the SUT runs against.
    Db,
}

impl ContainerRole {
    /// All roles, fixed order (schema emission derives from this).
    pub const ALL: &[ContainerRole] = &[ContainerRole::Sut, ContainerRole::Db];

    /// The display label (progress lines, summary tables) — the same token
    /// the wire serialization carries.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ContainerRole::Sut => "sut",
            ContainerRole::Db => "db",
        }
    }
}

/// The run phase a resource sample was taken in (closed vocabulary): the
/// charts shade warmup, and trailing samples taken while in-flight
/// completions drain past the planned window stamp as `drain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePhase {
    /// Before the measured window opened; not recorded in the verdict.
    Warmup,
    /// Inside the recorded measurement window.
    Measured,
    /// After the window closed, while in-flight completions drained.
    Drain,
}

impl ResourcePhase {
    /// All phases, run order (schema emission derives from this).
    pub const ALL: &[ResourcePhase] = &[
        ResourcePhase::Warmup,
        ResourcePhase::Measured,
        ResourcePhase::Drain,
    ];
}

/// One resource observation of one container at a run-clock offset (offsets
/// from the measured window's start — never wall-clock, the same determinism
/// rule as every other record field).
///
/// Peak and mean aggregates are DERIVED from the series at render time, never
/// stored beside it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSample {
    /// Seconds since the measured window started (warmup included).
    pub offset_s: u64,
    /// Which run phase the sample was taken in.
    pub phase: ResourcePhase,
    /// CPU utilisation percent over the preceding sample interval
    /// (100 = one full core).
    pub cpu_pct: f64,
    /// Resident-set memory, bytes.
    pub rss_bytes: u64,
    /// Cumulative block-device bytes read since container start (deltas →
    /// rates at render time).
    pub blk_read_bytes: u64,
    /// Cumulative block-device bytes written since container start.
    pub blk_write_bytes: u64,
    /// Cumulative network bytes received since container start.
    pub net_rx_bytes: u64,
    /// Cumulative network bytes transmitted since container start.
    pub net_tx_bytes: u64,
}

/// One container's sampled resource time-series.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerResourceSeries {
    /// Which container of the deployment this series describes.
    pub role: ContainerRole,
    /// The container-runtime identity sampled (the ixit `containers` block).
    pub name: String,
    /// The observations, in offset order.
    pub samples: Vec<ResourceSample>,
}

impl ContainerResourceSeries {
    /// Peak CPU% over the whole series (`0` when empty) — the one shared
    /// derivation the progress logs and the generated summaries both use
    /// (aggregates DERIVE from the series, never stored beside it).
    #[must_use]
    pub fn cpu_peak(&self) -> f64 {
        self.samples.iter().map(|s| s.cpu_pct).fold(0.0, f64::max)
    }

    /// Peak RSS bytes over the whole series (`0` when empty).
    #[must_use]
    pub fn rss_peak(&self) -> u64 {
        self.samples.iter().map(|s| s.rss_bytes).max().unwrap_or(0)
    }

    /// The measured-phase samples — falling back to the whole series when
    /// the window never reached the measured phase (an aborted run still
    /// reports what it saw).
    #[must_use]
    pub fn measured_samples(&self) -> Vec<&ResourceSample> {
        let measured: Vec<&ResourceSample> = self
            .samples
            .iter()
            .filter(|s| s.phase == ResourcePhase::Measured)
            .collect();
        if measured.is_empty() {
            self.samples.iter().collect()
        } else {
            measured
        }
    }
}

/// The database volume's on-disk size at the run's four anchors (bytes).
///
/// The first two yield bytes per committed composition (the
/// storage-efficiency headline); the last two yield the sustained load's
/// write amplification. Each anchor is honestly absent when it could not be
/// probed (a failed probe degrades to absence, never a run failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskAnchors {
    /// Before the scale seed (the empty-volume baseline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_scale_seed_bytes: Option<u64>,
    /// After the scale seed committed its compositions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_scale_seed_bytes: Option<u64>,
    /// After the pack preflight + standing-ward seed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_ward_seed_bytes: Option<u64>,
    /// After the measured window drained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_window_bytes: Option<u64>,
    /// Compositions the scale seed committed (the bytes-per-composition
    /// denominator — a run fact, not derivable from the series).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_compositions: Option<u64>,
}

/// The resource telemetry of one measured run.
///
/// Measured CONTEXT, never
/// verdict-bearing: classes stay earned on latency/error/throughput only,
/// and an absent record never fails a run (sampling is optional by
/// capability — it requires the ixit `containers` block and a reachable
/// container runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcesRecord {
    /// The fixed sampling cadence, seconds.
    pub sample_interval_s: u64,
    /// Per-container series (SUT and database separately).
    pub containers: Vec<ContainerResourceSeries>,
    /// The disk anchors — present on measured class runs (whose seeding
    /// milestones bracket them); absent on stress steps (exploration
    /// stays light).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskAnchors>,
}

/// The whole measured run for one performance case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    /// The performance case this run measured.
    pub case: CaseId,
    /// The volumetric class the case claims.
    pub class: PerfClass,
    /// The ixit environment block the run was measured in — mandatory:
    /// performance is meaningless without the deployment described, and an
    /// earned class is always reported WITH its environment.
    pub environment: crate::ixit::Environment,
    /// The offered load the schedule actually sustained (arrivals/s).
    pub offered_load_sustained: f64,
    /// The warmup the run honoured before recording (seconds).
    pub warmup_s: u64,
    /// The recorded (post-warmup) measurement window (seconds).
    pub duration_s: u64,
    /// Per-operation records, one per operation the workload drove.
    pub operations: Vec<OperationMeasurement>,
    /// The verdict — computed, never asserted; any consumer re-derives it
    /// from the decoded histograms + the case thresholds.
    pub verdict: ClassVerdict,
    /// The named threshold violations behind a `not-earned` verdict (empty
    /// when earned).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
    /// The resource telemetry sampled during the run — measured CONTEXT,
    /// never a verdict input; absent when the ixit declares no
    /// `containers` block or the container runtime was unreachable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesRecord>,
}

/// The one-line evidence behind a measured-run verdict.
///
/// It serves progress streams and console summaries — the committed record
/// (encoded histograms + environment) stays the re-checkable evidence; this is
/// the operator-facing digest of it.
#[must_use]
pub fn verdict_evidence(measurement: &Measurement) -> String {
    let floor = measurement.class.arrival_floor_per_s();
    let (requests, errors) = measurement
        .operations
        .iter()
        .fold((0_u64, 0_u64), |(r, e), op| {
            (r.saturating_add(op.requests), e.saturating_add(op.errors))
        });
    let worst = measurement
        .operations
        .iter()
        .max_by(|a, b| a.latency_ms_p99.total_cmp(&b.latency_ms_p99));
    let worst_text = worst.map_or_else(
        || "no operations measured".to_owned(),
        |op| format!("worst p99 {:.0} ms ({})", op.latency_ms_p99, op.operation),
    );
    let verdict_text = match measurement.verdict {
        ClassVerdict::Earned => "EARNED",
        ClassVerdict::NotEarned => "NOT EARNED",
    };
    let violation_text = if measurement.violations.is_empty() {
        String::new()
    } else {
        format!(" — {}", measurement.violations.join("; "))
    };
    format!(
        "window verdict: class {} {verdict_text} — offered {:.2}/s vs floor {floor}/s; \
         {worst_text}; {errors} errors / {requests} requests{violation_text}",
        measurement.class.token(),
        measurement.offered_load_sustained,
    )
}

/// Class verdicts (the second machinery's output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClassVerdict {
    /// Every threshold of the case held in the measured run.
    Earned,
    /// At least one threshold was violated.
    NotEarned,
}

/// The pure class-verdict function: every threshold of the case holds in the
/// single measured run ⇒ `earned`, else `not-earned`.
///
/// Latency metrics are re-derived from the DECODED histograms (never trusted
/// from the summary fields), which is what makes the record re-checkable.
///
/// # Errors
/// Returns a message when a threshold references an operation the run did
/// not measure, or a histogram fails to decode.
pub fn class_verdict(
    case: &PerformanceCase,
    offered_load_sustained: f64,
    operations: &[OperationMeasurement],
) -> Result<(ClassVerdict, Vec<String>), String> {
    let mut violations = Vec::new();
    for threshold in &case.thresholds {
        match threshold.metric {
            Metric::OfferedLoadSustained => {
                if let Some(min) = threshold.min
                    && offered_load_sustained < min
                {
                    violations.push(format!(
                        "offered_load_sustained {offered_load_sustained}/s < min {min}/s"
                    ));
                }
            }
            Metric::ErrorRate => {
                let (requests, errors) = operations.iter().fold((0_u64, 0_u64), |(r, e), m| {
                    (r.saturating_add(m.requests), e.saturating_add(m.errors))
                });
                let rate = if requests == 0 {
                    1.0
                } else {
                    #[expect(
                        clippy::as_conversions,
                        clippy::cast_precision_loss,
                        reason = "request counts << 2^52"
                    )]
                    {
                        errors as f64 / requests as f64
                    }
                };
                if let Some(max) = threshold.max
                    && rate > max
                {
                    violations.push(format!("error_rate {rate} > max {max}"));
                }
            }
            Metric::LatencyP50 | Metric::LatencyP90 | Metric::LatencyP99 => {
                let quantile = match threshold.metric {
                    Metric::LatencyP50 => 0.50,
                    Metric::LatencyP90 => 0.90,
                    _ => 0.99,
                };
                let targets: Vec<&OperationMeasurement> = match &threshold.operation {
                    Some(op) => {
                        let found: Vec<_> =
                            operations.iter().filter(|m| &m.operation == op).collect();
                        if found.is_empty() {
                            return Err(format!("threshold references unmeasured operation {op}"));
                        }
                        found
                    }
                    None => operations.iter().collect(),
                };
                for m in targets {
                    let histogram = m.decode_histogram()?;
                    let value_ms = us_to_ms(histogram.value_at_quantile(quantile));
                    if let Some(max) = threshold.max
                        && value_ms > max
                    {
                        violations.push(format!(
                            "{} {:?} {value_ms}ms > max {max}ms",
                            m.operation, threshold.metric
                        ));
                    }
                }
            }
        }
    }
    let verdict = if violations.is_empty() {
        ClassVerdict::Earned
    } else {
        ClassVerdict::NotEarned
    };
    Ok((verdict, violations))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(rate: &str) -> PerformanceCase {
        serde_saphyr::from_str(&format!(
            "id: PERF-hospital_sim-class_S\nkind: performance\ncomponent: PERFORMANCE\ndescription: d\ntest_purpose: t\nspec_refs: [\"CNF 2.0 performance schedule\"]\nclass: S\ncorpus: cnf.scale.100k\nworkload:\n  arrival_rate: {rate}\n  warmup: PT5M\n  duration: PT1H\n  journeys: {{ chart_review: 88%, vitals_round: 12% }}\nthresholds:\n  - {{ metric: latency_p99, operation: composition_read, max: 1000 }}\n  - {{ metric: error_rate, max: 0 }}\n  - {{ metric: offered_load_sustained, min: 15 }}\n"
        ))
        .unwrap()
    }

    fn catalogue() -> JourneyCatalogue {
        serde_saphyr::from_str(
            "chart_review:\n  description: ward-round chart reads\n  derivation: \"~597 EHR interactions per encounter (PMC10148376)\"\n  stages:\n    - { op: composition_read, at: PT0S }\n    - { op: adhoc_query, at: PT30S }\nvitals_round:\n  description: shift vitals\n  derivation: \"nursing observation rounds per bed-day\"\n  stages:\n    - { op: composition_commit, template: cnf.ckm.vital_signs, at: PT0S }\nmedication_round:\n  description: eMAR loop\n  derivation: \"21.8 prescription items/capita/yr (NHS BSA PCA 2024/25)\"\n  stages:\n    - { op: composition_commit, template: cnf.ckm.eprescription, at: PT0S }\n    - { op: composition_commit, template: cnf.ckm.eprescription, at: { periodic: { interval: PT20M, count: 2 } } }\nlab_pipeline:\n  description: order -> result\n  derivation: \"15 laboratory results/capita/yr (RCPath)\"\n  stages:\n    - { op: composition_commit, template: cnf.ckm.ereferral, at: PT0S }\n    - { op: contribution_commit, template: cnf.ckm.lab_result, at: { uniform: [PT20M, PT50M] } }\n    - { op: composition_read_current, at: PT1H }\n",
        )
        .unwrap()
    }

    fn histogram(values_us: &[u64]) -> Histogram<u64> {
        let mut h = Histogram::new(3).unwrap();
        for v in values_us {
            h.record(*v).unwrap();
        }
        h
    }

    #[test]
    fn the_class_s_case_parses_and_holds_its_floor() {
        let c = case("15/s");
        assert!(c.check_invariants().is_ok());
        assert!(case("40/s").check_invariants().is_ok());
        assert!(case("2/s").check_invariants().is_err()); // below the S floor
    }

    #[test]
    fn verdicts_recheck_from_the_encoded_histogram() {
        let c = case("15/s");
        // fast reads: p99 well under 1000ms
        let fast = histogram(&[10_000, 20_000, 30_000, 50_000]);
        let m = OperationMeasurement::from_histogram("composition_read", &fast, 0).unwrap();
        let decoded = m.decode_histogram().unwrap();
        assert_eq!(decoded.len(), 4);
        let (verdict, violations) = class_verdict(&c, 15.2, std::slice::from_ref(&m)).unwrap();
        assert_eq!(verdict, ClassVerdict::Earned);
        assert!(violations.is_empty());

        // a stalled tail: p99 ~5s -> not earned, violation named
        let slow = histogram(&[10_000, 20_000, 5_000_000, 5_100_000]);
        let m2 = OperationMeasurement::from_histogram("composition_read", &slow, 0).unwrap();
        let (verdict, violations) = class_verdict(&c, 15.2, &[m2]).unwrap();
        assert_eq!(verdict, ClassVerdict::NotEarned);
        assert!(violations.iter().any(|v| v.contains("LatencyP99")));

        // under-offered load
        let (verdict, violations) = class_verdict(&c, 12.0, &[m]).unwrap();
        assert_eq!(verdict, ClassVerdict::NotEarned);
        assert!(
            violations
                .iter()
                .any(|v| v.contains("offered_load_sustained"))
        );
    }

    #[test]
    fn error_rate_and_tampered_summaries_cannot_hide() {
        let c = case("15/s");
        let h = histogram(&[10_000; 8]);
        let mut m = OperationMeasurement::from_histogram("composition_read", &h, 1).unwrap();
        let (verdict, _) = class_verdict(&c, 15.0, &[m.clone()]).unwrap();
        assert_eq!(verdict, ClassVerdict::NotEarned); // one error breaks error_rate 0

        // Tamper the SUMMARY p99 — the verdict still re-derives from the
        // histogram, so the tamper cannot flip it.
        m.errors = 0;
        m.latency_ms_p99 = 0.001;
        let slow = histogram(&[5_000_000; 8]);
        let mut buffer = Vec::new();
        V2Serializer::new().serialize(&slow, &mut buffer).unwrap();
        m.hdr_v2_base64 = base64::engine::general_purpose::STANDARD.encode(&buffer);
        let (verdict, violations) = class_verdict(&c, 15.0, &[m]).unwrap();
        assert_eq!(verdict, ClassVerdict::NotEarned);
        assert!(!violations.is_empty());
    }

    #[test]
    fn the_resources_block_is_optional_and_round_trips() {
        // A pre-telemetry record (no `resources`) still parses — the
        // committed baseline stays valid.
        let h = histogram(&[10_000]);
        let op = OperationMeasurement::from_histogram("ehr_read", &h, 0).unwrap();
        let mut m = Measurement {
            case: CaseId::parse("PERF-hospital_sim-class_POC").unwrap(),
            class: PerfClass::Poc,
            environment: serde_json::from_value(serde_json::json!({
                "hardware_class": "test", "cores": 1, "memory_gb": 1,
                "storage_class": "ram", "topology": "stub"
            }))
            .unwrap(),
            offered_load_sustained: 2.0,
            warmup_s: 300,
            duration_s: 3600,
            operations: vec![op],
            verdict: ClassVerdict::Earned,
            violations: Vec::new(),
            resources: None,
        };
        let bare = serde_json::to_value(&m).unwrap();
        assert!(bare.get("resources").is_none()); // absent, never null
        let parsed: Measurement = serde_json::from_value(bare).unwrap();
        assert!(parsed.resources.is_none());

        m.resources = Some(ResourcesRecord {
            sample_interval_s: 10,
            containers: vec![ContainerResourceSeries {
                role: ContainerRole::Sut,
                name: "ferroehr-ferroehr-1".to_owned(),
                samples: vec![ResourceSample {
                    offset_s: 10,
                    phase: ResourcePhase::Warmup,
                    cpu_pct: 42.5,
                    rss_bytes: 123_456_789,
                    blk_read_bytes: 1_000,
                    blk_write_bytes: 2_000,
                    net_rx_bytes: 3_000,
                    net_tx_bytes: 4_000,
                }],
            }],
            disk: Some(DiskAnchors {
                before_scale_seed_bytes: Some(10),
                after_scale_seed_bytes: Some(20),
                after_ward_seed_bytes: None,
                after_window_bytes: Some(30),
                seed_compositions: Some(1_000_000),
            }),
        });
        let full = serde_json::to_value(&m).unwrap();
        // Run-clock offsets + phase stamps on the wire, absent anchors omitted.
        let sample = &full["resources"]["containers"][0]["samples"][0];
        assert_eq!(sample["offset_s"], 10);
        assert_eq!(sample["phase"], "warmup");
        assert_eq!(full["resources"]["containers"][0]["role"], "sut");
        assert!(
            full["resources"]["disk"]
                .get("after_ward_seed_bytes")
                .is_none()
        );
        let parsed: Measurement = serde_json::from_value(full).unwrap();
        let resources = parsed.resources.unwrap();
        assert_eq!(resources.sample_interval_s, 10);
        assert_eq!(resources.disk.unwrap().seed_compositions, Some(1_000_000));
    }

    #[test]
    fn durations_and_rates_round_trip() {
        assert_eq!(parse_iso_duration_secs("PT5M"), Some(300));
        assert_eq!(parse_iso_duration_secs("PT1H"), Some(3600));
        assert_eq!(parse_iso_duration_secs("PT1H30M15S"), Some(5415));
        assert_eq!(parse_iso_duration_secs("P1D"), None);
        assert!((PerfClass::R.arrival_floor_per_s() - 1_500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_operation_vocabulary_is_closed_and_classified() {
        for op in PerfOp::ALL {
            assert_eq!(PerfOp::parse(op.as_str()).unwrap(), *op);
        }
        assert!(PerfOp::parse("delete_everything").is_err());
        assert!(PerfOp::CompositionCommit.is_write());
        assert!(!PerfOp::CompositionRead.is_write());
        assert!(PerfOp::ContributionCommit.needs_template());
        assert!(!PerfOp::DirectoryRead.needs_template());
        // Every operation exercises at least one claimed capability (the
        // certificate's workload-coverage join has no unmapped labels), and
        // every token is distinct (the measurement labels key on them).
        let mut tokens: Vec<&str> = Vec::new();
        for op in PerfOp::ALL {
            assert!(!op.capabilities().is_empty(), "{} unmapped", op.as_str());
            assert!(
                !tokens.contains(&op.as_str()),
                "duplicate token {}",
                op.as_str()
            );
            tokens.push(op.as_str());
        }
        // A DENIED write mutates nothing, so it is not a write arrival.
        assert!(!PerfOp::ReadonlyWriteDenied.is_write());
        assert!(PerfOp::ReadonlyWriteDenied.needs_template());
        assert!(PerfOp::CompositionCommitFlat.is_write());
        assert!(!PerfOp::CompositionCommitFlat.needs_template()); // the FLAT payload is auxiliary
        // The boundary/platform operations are the ONLY non-primary ones.
        for op in PerfOp::ALL {
            let non_primary = matches!(
                op,
                PerfOp::UnauthenticatedProbe
                    | PerfOp::ReadonlyWriteDenied
                    | PerfOp::SmartConfigurationRead
                    | PerfOp::AdminContributionReport
            );
            assert_eq!(
                op.principal() == Principal::Primary,
                !non_primary,
                "{} principal",
                op.as_str()
            );
        }
        // Auxiliary payloads are declared exactly by the operations that
        // carry one (the pack loads them on that signal).
        assert_eq!(
            PerfOp::PartyCreate.aux_payload(),
            Some(AuxPayloadKind::Person)
        );
        assert_eq!(PerfOp::CompositionCommit.aux_payload(), None);
    }

    #[test]
    fn stage_offsets_parse_all_three_forms() {
        let cat = catalogue();
        cat.check_invariants().unwrap();
        let meds = cat.get("medication_round").unwrap();
        assert_eq!(meds.arrivals(), 3); // order + 2 periodic administrations
        assert_eq!(meds.max_offset_s(), 1200); // (count-1) * 20 min
        let lab = cat.get("lab_pipeline").unwrap();
        assert_eq!(lab.arrivals(), 3);
        assert_eq!(lab.max_offset_s(), 3600);
        assert_eq!(
            lab.stages[1].at,
            StageOffset::Uniform {
                min_s: 1200,
                max_s: 3000
            }
        );
    }

    #[test]
    fn catalogue_invariants_reject_bad_stages() {
        // A commit without a template.
        let bad: JourneyCatalogue = serde_saphyr::from_str(
            "j:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_commit, at: PT0S }\n",
        )
        .unwrap();
        assert!(bad.check_invariants().unwrap_err().contains("template"));
        // A template on a read.
        let bad: JourneyCatalogue = serde_saphyr::from_str(
            "j:\n  description: d\n  derivation: g\n  stages:\n    - { op: ehr_read, template: x, at: PT0S }\n",
        )
        .unwrap();
        assert!(
            bad.check_invariants()
                .unwrap_err()
                .contains("must not carry")
        );
        // An unknown operation.
        let bad: JourneyCatalogue = serde_saphyr::from_str(
            "j:\n  description: d\n  derivation: g\n  stages:\n    - { op: drop_tables, at: PT0S }\n",
        )
        .unwrap();
        assert!(bad.check_invariants().is_err());
    }

    #[test]
    fn expansion_reconciles_the_population_envelope() {
        let cat = catalogue();
        // 88% chart_review (2 reads) + 12% vitals (1 write):
        // arrivals/journey = 0.88*2 + 0.12 = 1.88; write share = 12/188.
        let shares = vec![
            ("chart_review".to_owned(), Percent(88.0)),
            ("vitals_round".to_owned(), Percent(12.0)),
        ];
        let expansion = cat.expansion(&shares).unwrap();
        assert!((expansion.arrivals_per_journey - 1.88).abs() < 1e-9);
        assert!((expansion.write_share - 12.0 / 1.88).abs() < 1e-9);
        let total: f64 = expansion.op_shares.iter().map(|(_, s)| s).sum();
        assert!((total - 100.0).abs() < 1e-9);

        // All-reads breaks the band (write share 0 < 10:1..50:1 floor).
        let all_reads = vec![("chart_review".to_owned(), Percent(100.0))];
        assert!(
            cat.expansion(&all_reads)
                .unwrap_err()
                .contains("derivation band")
        );
        // Write-heavy breaks the band the other way.
        let all_writes = vec![("vitals_round".to_owned(), Percent(100.0))];
        assert!(
            cat.expansion(&all_writes)
                .unwrap_err()
                .contains("derivation band")
        );
        // Unknown journey named.
        let unknown = vec![("teleportation".to_owned(), Percent(100.0))];
        assert!(cat.expansion(&unknown).unwrap_err().contains("unknown"));
    }

    #[test]
    fn diurnal_requires_an_extended_hold() {
        let mut c = case("15/s");
        c.workload.arrival_curve = ArrivalCurve::Diurnal;
        assert!(c.check_invariants().unwrap_err().contains("PT8H"));
        c.workload.duration = WorkloadDuration(8 * 3600);
        assert!(c.check_invariants().is_ok());
    }

    /// The performance ladder is closed and each rung round-trips its token,
    /// so a class can only enter a claim or a case by its published spelling.
    #[test]
    fn the_performance_ladder_round_trips_and_stays_closed() {
        for class in PerfClass::ALL {
            assert_eq!(PerfClass::parse(class.token()), Ok(*class));
        }
        assert_eq!(
            PerfClass::parse("XL"),
            Err("unknown performance class \"XL\"".to_owned())
        );
        // The ladder is ascending, which is what makes its index the rank.
        let floors: Vec<f64> = PerfClass::ALL
            .iter()
            .map(|c| c.arrival_floor_per_s())
            .collect();
        assert!(floors.windows(2).all(|w| w[0] < w[1]), "{floors:?}");
    }

    /// The workload scalars carry their unit in the authored text, so each one
    /// renders back to the form the artifact was written in. A rate that lost
    /// its `/s`, or a duration that re-rendered as a different length, would
    /// silently redefine the offered load.
    #[test]
    fn the_workload_scalars_render_back_to_their_authored_forms() {
        #[derive(Debug, PartialEq, Deserialize, Serialize)]
        struct Carrier {
            rate: RatePerSecond,
            warmup: WorkloadDuration,
            share: Percent,
        }
        let carrier: Carrier =
            serde_saphyr::from_str("rate: 15/s\nwarmup: PT1H30M\nshare: 52.6%\n").unwrap();
        assert_eq!(carrier.rate, RatePerSecond(15.0));
        assert_eq!(carrier.warmup, WorkloadDuration(5400));
        assert_eq!(carrier.share, Percent(52.6));
        assert_eq!(
            serde_json::to_value(&carrier).unwrap(),
            serde_json::json!({ "rate": "15/s", "warmup": "PT1H30M", "share": "52.6%" })
        );

        // A whole-second duration keeps its `S` component, and a zero-length
        // one still renders a component rather than a bare `PT`.
        assert_eq!(
            serde_json::to_value(WorkloadDuration(0)).unwrap(),
            serde_json::json!("PT0S")
        );
        assert_eq!(
            serde_json::to_value(WorkloadDuration(90)).unwrap(),
            serde_json::json!("PT1M30S")
        );
    }

    /// Each scalar names the unit it wanted when the text lacks it, so an
    /// unqualified number never enters a workload as a silent default.
    #[test]
    fn a_workload_scalar_without_its_unit_is_refused() {
        for (yaml, wanted) in [
            ("rate: 15\n", "must end in /s"),
            ("warmup: 90\n", "is not PT[nH][nM][nS]"),
            ("share: 52.6\n", "must end in %"),
        ] {
            #[derive(Deserialize)]
            struct Scalars {
                #[serde(default)]
                rate: Option<RatePerSecond>,
                #[serde(default)]
                warmup: Option<WorkloadDuration>,
                #[serde(default)]
                share: Option<Percent>,
            }
            let error = serde_saphyr::from_str::<Scalars>(yaml)
                .map(|s| (s.rate.is_some(), s.warmup.is_some(), s.share.is_some()))
                .expect_err("an unqualified scalar is refused");
            assert!(error.to_string().contains(wanted), "{error}");
        }
        // A duration carrying an unknown designator is refused too, and one
        // whose components overflow a second count is refused rather than
        // wrapping into a shorter window than the artifact authored.
        assert!(serde_saphyr::from_str::<WorkloadDuration>("PT3Y\n").is_err());
        assert!(serde_saphyr::from_str::<WorkloadDuration>("PT18446744073709551615S1S\n").is_err());
    }

    /// A performance case states the four things a measured run needs from
    /// it: the `performance` kind, the `PERFORMANCE` chapter, journey shares
    /// that sum to 100%, and a bound on every threshold. Each refusal names
    /// the rule it violated.
    #[test]
    fn a_performance_case_refuses_every_shape_that_measures_nothing() {
        let mut c = case("15/s");
        c.kind = "functional".to_owned();
        assert_eq!(
            c.check_invariants(),
            Err("kind must be `performance`, got \"functional\"".to_owned())
        );

        let mut c = case("15/s");
        c.component = "EHR".to_owned();
        assert_eq!(
            c.check_invariants(),
            Err("component must be PERFORMANCE, got \"EHR\"".to_owned())
        );

        let mut c = case("15/s");
        c.workload.journeys = vec![("chart_review".to_owned(), Percent(60.0))];
        assert_eq!(
            c.check_invariants(),
            Err("workload journeys sum to 60%, must be 100%".to_owned())
        );

        let mut c = case("15/s");
        c.thresholds.push(Threshold {
            metric: Metric::LatencyP50,
            operation: None,
            max: None,
            min: None,
        });
        assert_eq!(
            c.check_invariants(),
            Err("threshold carries neither max nor min".to_owned())
        );
    }

    /// The journey catalogue refuses the shapes that decompose nothing: an
    /// empty catalogue, and a journey with no stages.
    #[test]
    fn a_journey_catalogue_refuses_an_empty_decomposition() {
        let empty: JourneyCatalogue = serde_saphyr::from_str("{}\n").unwrap();
        assert_eq!(
            empty.check_invariants(),
            Err("journey catalogue is empty".to_owned())
        );
        let stageless: JourneyCatalogue =
            serde_saphyr::from_str("chart_review: { description: d, derivation: x, stages: [] }\n")
                .unwrap();
        assert_eq!(
            stageless.check_invariants(),
            Err("journey chart_review has no stages".to_owned())
        );
    }

    /// A workload that names no journey, or whose shares do not reconcile,
    /// expands to nothing — the expansion says which rule failed rather than
    /// returning an empty schedule.
    #[test]
    fn an_expansion_over_no_journeys_names_the_rule_it_failed() {
        let cat = catalogue();
        assert_eq!(
            cat.expansion(&[]).unwrap_err(),
            "workload journeys is empty"
        );
        let short = vec![("chart_review".to_owned(), Percent(60.0))];
        assert_eq!(
            cat.expansion(&short).unwrap_err(),
            "journey shares sum to 60%, must be 100%"
        );
    }

    /// A run that recorded no request at all reads as a 100% error rate,
    /// never as a clean zero: an empty window evidences nothing, and a
    /// divide-by-zero default of 0.0 would publish it as perfect.
    #[test]
    fn a_window_with_no_requests_reads_as_a_total_error_rate() {
        let mut c = case("15/s");
        c.thresholds = vec![Threshold {
            metric: Metric::ErrorRate,
            operation: None,
            max: Some(0.0),
            min: None,
        }];
        let (verdict, violations) = class_verdict(&c, 15.2, &[]).unwrap();
        assert_eq!(verdict, ClassVerdict::NotEarned);
        assert!(
            violations.iter().any(|v| v.starts_with("error_rate 1 >")),
            "{violations:?}"
        );
    }

    /// Each latency metric reads its own quantile out of the same histogram,
    /// so a p50 bound and a p99 bound over one recording judge different
    /// values.
    #[test]
    fn each_latency_metric_reads_its_own_quantile() {
        let mut c = case("15/s");
        let h = histogram(&[
            10_000, 10_000, 10_000, 10_000, 10_000, 10_000, 10_000, 900_000,
        ]);
        let m = OperationMeasurement::from_histogram("composition_read", &h, 0).unwrap();
        // p50 is 10 ms, p90/p99 reach the 900 ms tail: a 100 ms bound holds
        // at p50 and breaks at p90 and p99.
        for (metric, breaches) in [
            (Metric::LatencyP50, false),
            (Metric::LatencyP90, true),
            (Metric::LatencyP99, true),
        ] {
            c.thresholds = vec![Threshold {
                metric,
                operation: Some("composition_read".to_owned()),
                max: Some(100.0),
                min: None,
            }];
            let (verdict, violations) = class_verdict(&c, 15.2, std::slice::from_ref(&m)).unwrap();
            assert_eq!(
                verdict == ClassVerdict::NotEarned,
                breaches,
                "{metric:?}: {violations:?}"
            );
        }
    }

    /// The one-line verdict evidence names the class, the verdict, the
    /// offered load against its floor, the worst operation's p99 and the
    /// error count — and appends the violations behind a `not-earned`.
    #[test]
    fn the_verdict_evidence_line_carries_the_numbers_behind_it() {
        let h = histogram(&[40_000]);
        let op = OperationMeasurement::from_histogram("composition_read", &h, 2).unwrap();
        let mut m = Measurement {
            case: CaseId::parse("PERF-hospital_sim-class_POC").unwrap(),
            class: PerfClass::Poc,
            environment: serde_json::from_value(serde_json::json!({
                "hardware_class": "test", "cores": 1, "memory_gb": 1,
                "storage_class": "ram", "topology": "stub"
            }))
            .unwrap(),
            offered_load_sustained: 2.02,
            warmup_s: 300,
            duration_s: 3600,
            operations: vec![op],
            verdict: ClassVerdict::Earned,
            violations: Vec::new(),
            resources: None,
        };
        let earned = verdict_evidence(&m);
        assert!(earned.contains("class POC EARNED"), "{earned}");
        assert!(earned.contains("offered 2.02/s vs floor 2/s"), "{earned}");
        assert!(
            earned.contains("worst p99 40 ms (composition_read)"),
            "{earned}"
        );
        assert!(earned.contains("2 errors / 1 requests"), "{earned}");

        m.verdict = ClassVerdict::NotEarned;
        m.violations = vec!["error_rate 2 > max 0".to_owned()];
        let not_earned = verdict_evidence(&m);
        assert!(not_earned.contains("class POC NOT EARNED"), "{not_earned}");
        assert!(
            not_earned.ends_with(" — error_rate 2 > max 0"),
            "{not_earned}"
        );

        // A window that measured no operation says so rather than printing a
        // p99 it never observed.
        m.operations.clear();
        assert!(
            verdict_evidence(&m).contains("no operations measured"),
            "{}",
            verdict_evidence(&m)
        );
    }

    /// The resource aggregates DERIVE from the series, so an empty series
    /// reads as zero peaks, and a window that never reached the measured
    /// phase still reports the samples it did take.
    #[test]
    fn the_resource_aggregates_derive_from_the_series() {
        let sample = |offset_s, phase, cpu_pct, rss_bytes| ResourceSample {
            offset_s,
            phase,
            cpu_pct,
            rss_bytes,
            blk_read_bytes: 0,
            blk_write_bytes: 0,
            net_rx_bytes: 0,
            net_tx_bytes: 0,
        };
        let empty = ContainerResourceSeries {
            role: ContainerRole::Sut,
            name: "sut".to_owned(),
            samples: Vec::new(),
        };
        assert!(empty.cpu_peak().abs() < f64::EPSILON);
        assert_eq!(empty.rss_peak(), 0);
        assert!(empty.measured_samples().is_empty());

        let aborted = ContainerResourceSeries {
            role: ContainerRole::Sut,
            name: "sut".to_owned(),
            samples: vec![
                sample(10, ResourcePhase::Warmup, 42.5, 100),
                sample(20, ResourcePhase::Warmup, 11.0, 300),
            ],
        };
        assert!((aborted.cpu_peak() - 42.5).abs() < f64::EPSILON);
        assert_eq!(aborted.rss_peak(), 300);
        // No measured-phase sample: the whole series is reported.
        assert_eq!(aborted.measured_samples().len(), 2);

        let complete = ContainerResourceSeries {
            role: ContainerRole::Sut,
            name: "sut".to_owned(),
            samples: vec![
                sample(10, ResourcePhase::Warmup, 42.5, 100),
                sample(20, ResourcePhase::Measured, 11.0, 300),
            ],
        };
        assert_eq!(complete.measured_samples().len(), 1);
    }

    /// A stage offset renders back to the shape it was authored in, and both
    /// structured forms refuse the values that describe no schedule: an
    /// inverted uniform window and a periodic repeat of zero.
    #[test]
    fn a_stage_offset_round_trips_and_refuses_an_empty_schedule() {
        for (yaml, json) in [
            ("PT30S\n", serde_json::json!("PT30S")),
            (
                "uniform: [PT10S, PT30S]\n",
                serde_json::json!({ "uniform": ["PT10S", "PT30S"] }),
            ),
            (
                "periodic: { interval: PT5M, count: 3 }\n",
                serde_json::json!({ "periodic": { "interval": "PT5M", "count": 3 } }),
            ),
        ] {
            let offset: StageOffset = serde_saphyr::from_str(yaml).unwrap();
            assert_eq!(serde_json::to_value(offset).unwrap(), json);
        }
        let inverted = serde_saphyr::from_str::<StageOffset>("uniform: [PT30S, PT10S]\n")
            .expect_err("an inverted window schedules nothing");
        assert!(inverted.to_string().contains("is inverted"), "{inverted}");
        let empty =
            serde_saphyr::from_str::<StageOffset>("periodic: { interval: PT5M, count: 0 }\n")
                .expect_err("a zero repeat schedules nothing");
        assert!(
            empty.to_string().contains("periodic count must be >= 1"),
            "{empty}"
        );
    }
}
