use std::collections::{BTreeMap, BTreeSet};

use crate::{
    JjItem, JjMech, JjTrait,
    stock_template::{StockTemplateLoadout, StockTemplateTypes},
};

mod yet_another_legendary_mech;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModIdentity {
    pub(crate) folder_name: String,
    pub(crate) display_name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) build_number: Option<String>,
    pub(crate) steam_published_file_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
// The complete read-only view is intentionally available to future processors.
// Fields become live independently as processors are registered.
#[allow(dead_code)]
pub(crate) struct SourceView<'a> {
    pub(crate) identity: &'a ModIdentity,
    pub(crate) source_id: &'a str,
    pub(crate) load_order: i32,
    pub(crate) items: &'a [JjItem],
    pub(crate) mechs: &'a [JjMech],
    pub(crate) stock_template_types: &'a BTreeMap<String, StockTemplateTypes>,
    pub(crate) stock_templates: &'a [StockTemplateLoadout],
    pub(crate) traits: &'a [JjTrait],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpectedMechPresentation {
    pub(crate) chassis: String,
    pub(crate) tons: Option<u16>,
    pub(crate) is_hero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MechPresentation {
    pub(crate) chassis: String,
    pub(crate) tons: Option<u16>,
    pub(crate) hero_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MechPresentationContribution {
    pub(crate) variant: String,
    pub(crate) expected: ExpectedMechPresentation,
    pub(crate) replacement: MechPresentation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CompatibilityPlan {
    pub(crate) mech_presentations: Vec<MechPresentationContribution>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceCompatibility {
    mech_presentations: BTreeMap<String, MechPresentation>,
}

impl SourceCompatibility {
    pub(crate) fn presentation_for(&self, variant: &str) -> Option<&MechPresentation> {
        self.mech_presentations.get(variant)
    }
}

type ProcessFn = for<'a> fn(&SourceView<'a>) -> Result<CompatibilityPlan, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompatibilitySkip {
    pub(crate) processor_id: String,
    pub(crate) detected_version: Option<String>,
    pub(crate) detected_build_number: Option<String>,
    pub(crate) supported_version: String,
    pub(crate) supported_build_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompatibilityOutcome {
    NoMatch,
    Applied(SourceCompatibility),
    SkippedVersionBuild(CompatibilitySkip),
}

#[derive(Clone, Copy)]
pub(crate) struct ModSelector {
    folder_names: &'static [&'static str],
    steam_published_file_id: Option<&'static str>,
}

impl ModSelector {
    #[allow(dead_code)]
    pub(crate) const fn new(
        folder_names: &'static [&'static str],
        steam_published_file_id: Option<&'static str>,
    ) -> Self {
        Self {
            folder_names,
            steam_published_file_id,
        }
    }

    fn matches(self, identity: &ModIdentity) -> bool {
        if self.folder_names.is_empty() && self.steam_published_file_id.is_none() {
            return false;
        }
        if !self.folder_names.is_empty()
            && !self
                .folder_names
                .iter()
                .any(|folder_name| folder_name.eq_ignore_ascii_case(&identity.folder_name))
        {
            return false;
        }
        if let Some(expected_id) = self.steam_published_file_id
            && identity.steam_published_file_id.as_deref() != Some(expected_id)
        {
            return false;
        }
        true
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ProcessorRegistration {
    id: &'static str,
    selector: ModSelector,
    supported_version: &'static str,
    supported_build_number: &'static str,
    process: ProcessFn,
}

impl ProcessorRegistration {
    #[allow(dead_code)]
    pub(crate) const fn new(
        id: &'static str,
        selector: ModSelector,
        supported_version: &'static str,
        supported_build_number: &'static str,
        process: ProcessFn,
    ) -> Self {
        Self {
            id,
            selector,
            supported_version,
            supported_build_number,
            process,
        }
    }
}

// Mod-specific modules expose one registration for this auditable, compiled-in
// list. A source can match at most one processor.
const PROCESSORS: &[ProcessorRegistration] = &[yet_another_legendary_mech::REGISTRATION];

pub(crate) fn registered_processors() -> &'static [ProcessorRegistration] {
    PROCESSORS
}

pub(crate) fn process_source(
    source: &SourceView<'_>,
    processors: &[ProcessorRegistration],
) -> Result<CompatibilityOutcome, String> {
    let matching = processors
        .iter()
        .filter(|processor| processor.selector.matches(source.identity))
        .collect::<Vec<_>>();

    let Some(processor) = matching.first().copied() else {
        return Ok(CompatibilityOutcome::NoMatch);
    };
    if matching.len() > 1 {
        let processor_ids = matching
            .iter()
            .map(|processor| processor.id)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "multiple compatibility processors matched: {processor_ids}"
        ));
    }

    if source.identity.version.as_deref() != Some(processor.supported_version)
        || source.identity.build_number.as_deref() != Some(processor.supported_build_number)
    {
        return Ok(CompatibilityOutcome::SkippedVersionBuild(
            CompatibilitySkip {
                processor_id: processor.id.to_string(),
                detected_version: source.identity.version.clone(),
                detected_build_number: source.identity.build_number.clone(),
                supported_version: processor.supported_version.to_string(),
                supported_build_number: processor.supported_build_number.to_string(),
            },
        ));
    }

    let plan = (processor.process)(source)
        .map_err(|reason| format!("processor `{}` rejected the source: {reason}", processor.id))?;
    validate_plan(source, processor.id, plan).map(CompatibilityOutcome::Applied)
}

fn validate_plan(
    source: &SourceView<'_>,
    processor_id: &str,
    plan: CompatibilityPlan,
) -> Result<SourceCompatibility, String> {
    let mut seen_presentation_variants = BTreeSet::new();
    let mut mech_presentations = BTreeMap::new();

    for contribution in plan.mech_presentations {
        validate_text("target variant", &contribution.variant)?;
        let variant_key = contribution.variant.to_ascii_lowercase();
        if !seen_presentation_variants.insert(variant_key) {
            return Err(format!(
                "processor `{processor_id}` returned duplicate presentation contributions for variant `{}`",
                contribution.variant
            ));
        }

        let mech = validate_expected_mech(
            source,
            processor_id,
            &contribution.variant,
            &contribution.expected,
        )?;

        validate_text("replacement chassis", &contribution.replacement.chassis)?;
        if let Some(hero_name) = &contribution.replacement.hero_name {
            validate_text("replacement hero name", hero_name)?;
            if !mech.is_hero {
                return Err(format!(
                    "processor `{processor_id}` supplied a hero name for non-Hero variant `{}`",
                    contribution.variant
                ));
            }
        }
        mech_presentations.insert(contribution.variant, contribution.replacement);
    }

    Ok(SourceCompatibility { mech_presentations })
}

fn validate_expected_mech<'a>(
    source: &SourceView<'a>,
    processor_id: &str,
    variant: &str,
    expected: &ExpectedMechPresentation,
) -> Result<&'a JjMech, String> {
    let matching_mechs = source
        .mechs
        .iter()
        .filter(|mech| mech.variant == variant)
        .collect::<Vec<_>>();
    let [mech] = matching_mechs.as_slice() else {
        return Err(format!(
            "processor `{processor_id}` expected exactly one source mech with variant `{variant}`, found {}",
            matching_mechs.len()
        ));
    };
    if mech.chassis != expected.chassis
        || mech.tons != expected.tons
        || mech.is_hero != expected.is_hero
    {
        return Err(format!(
            "processor `{processor_id}` found stale source evidence for variant `{variant}`"
        ));
    }
    Ok(mech)
}

fn validate_text(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.trim() != value {
        return Err(format!("{field} must be nonempty and trimmed"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(())
}
