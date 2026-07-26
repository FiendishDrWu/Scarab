use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, BufReader, Cursor, Write},
    path::{Path, PathBuf},
};

use unreal_asset::{
    Asset,
    engine_version::EngineVersion,
    exports::{ExportBaseTrait, ExportNormalTrait, base_export::BaseExport},
    properties::{Property, PropertyDataTrait},
};

use crate::{
    ScanAccumulator, ScarabError, asset_registry,
    scanner::{self, CatalogAssetKind},
    stock_template, trait_catalog,
    unreal_name::render_fname,
};

const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
// The live MW5 registry is validated below this independent, read-only input bound.
const MAX_ASSET_REGISTRY_BYTES: usize = 256 * 1024 * 1024;

pub(crate) fn scan_pak_file(
    pak_path: &Path,
    scan: &mut ScanAccumulator,
) -> Result<(), ScarabError> {
    let mut pak = PakFile::open(pak_path)?;
    let files = pak.list_files();
    scan.files_scanned += files.len();

    let index = PakAssetIndex::new(&files);
    let mut registry_paths = files
        .iter()
        .filter(|path| is_asset_registry_path(path))
        .cloned()
        .collect::<Vec<_>>();
    registry_paths.sort_by_key(|path| (path.to_ascii_lowercase(), path.clone()));

    let mut registry_candidates = Vec::new();
    for registry_path in registry_paths {
        let Ok(bytes) = pak.read_file(&registry_path, MAX_ASSET_REGISTRY_BYTES) else {
            continue;
        };
        let Ok(mut candidates) = asset_registry::parse_registry(bytes) else {
            continue;
        };
        registry_candidates.append(&mut candidates);
    }

    let candidates = select_catalog_packages(&index, registry_candidates);

    for candidate in candidates {
        let uasset_path = candidate.path;
        let stem = normalized_stem(&uasset_path, ".uasset");
        let uasset = pak.read_file(&uasset_path, MAX_ASSET_BYTES)?;
        let uexp = match index.uexp_by_stem.get(&stem) {
            Some(uexp_path) => Some(pak.read_file(uexp_path, MAX_ASSET_BYTES)?),
            None => None,
        };
        let parsed = parse_catalog_asset(
            &uasset_path,
            uasset,
            uexp,
            candidate.kinds.contains(&CatalogAssetKind::Item),
        )?;
        scan.items.extend(parsed.items);
        scan.mechs.extend(parsed.mechs);
        for stock_types in parsed.stock_template_types {
            scan.stock_template_types
                .insert(stock_types.key.clone(), stock_types);
        }
        for loadout in parsed.stock_templates {
            scan.stock_templates.push(loadout);
        }
        scan.traits.extend(parsed.traits);
    }

    Ok(())
}

#[derive(Debug, Default)]
struct ParsedCatalogAsset {
    items: Vec<scanner::JjItem>,
    mechs: Vec<scanner::JjMech>,
    stock_template_types: Vec<stock_template::StockTemplateTypes>,
    stock_templates: Vec<stock_template::StockTemplateLoadout>,
    traits: Vec<trait_catalog::JjTrait>,
}

fn parse_catalog_asset(
    package_path: &str,
    uasset: Vec<u8>,
    uexp: Option<Vec<u8>>,
    collect_items: bool,
) -> Result<ParsedCatalogAsset, ScarabError> {
    let asset = Asset::new(
        Cursor::new(uasset),
        uexp.map(Cursor::new),
        EngineVersion::VER_UE4_26,
    )
    .map_err(|error| ScarabError::ParseAsset {
        path: package_path.to_string(),
        reason: error.to_string(),
    })?;

    let mut parsed = ParsedCatalogAsset::default();
    for export in &asset.asset_data.exports {
        let base_export = export.get_base_export();
        let class_name = base_export
            .get_class_type_for_ancestry(&asset)
            .get_content();
        let Some(asset_name) = catalog_asset_name(base_export) else {
            continue;
        };
        let Some(normal_export) = export.get_normal_export() else {
            continue;
        };

        if collect_items && let Some(category) = scanner::item_category_from_class(&class_name) {
            if scanner::is_jj_inventory_asset_name(&asset_name) {
                parsed.items.push(scanner::JjItem {
                    category,
                    asset_name,
                    data_asset_type: class_name,
                });
            }
        } else if class_name.to_ascii_lowercase().contains("mwmechdataasset") {
            let Some(variant) = scanner::variant_from_asset_name(&asset_name) else {
                continue;
            };
            if !scanner::is_jj_addable_mech_variant(&variant) {
                continue;
            }
            let Some(chassis) = find_chassis_in_properties(&normal_export.properties, &variant)
                .or_else(|| {
                    scanner::chassis_from_asset_path_with_variant(package_path, Some(&variant))
                })
            else {
                continue;
            };
            parsed.mechs.push(scanner::JjMech {
                chassis,
                variant,
                tons: find_tons_in_properties(&normal_export.properties),
                is_hero: find_bool_property_by_exact_name(&normal_export.properties, "bIsHeroMech")
                    .unwrap_or(false),
            });
            if let Some(stock_types) =
                stock_template::stock_types_from_mda(&asset_name, &normal_export.properties)
            {
                parsed.stock_template_types.push(stock_types);
            }
        } else if class_name
            .to_ascii_lowercase()
            .contains("mwmechloadoutasset")
            && let Some(loadout) =
                stock_template::stock_template_from_loadout(&asset_name, &normal_export.properties)
        {
            parsed.stock_templates.push(loadout);
        } else if let Some(catalog_trait) = trait_catalog::trait_from_unreal_export(
            &class_name,
            &asset_name,
            &normal_export.properties,
        ) {
            parsed.traits.push(catalog_trait);
        }
    }
    Ok(parsed)
}

#[derive(Debug)]
struct PakAssetIndex {
    uasset_by_alias: BTreeMap<String, BTreeSet<String>>,
    uexp_by_stem: BTreeMap<String, String>,
}

impl PakAssetIndex {
    fn new(files: &[String]) -> Self {
        let mut uasset_by_alias = BTreeMap::<String, BTreeSet<String>>::new();
        let mut uexp_by_stem = BTreeMap::new();
        for path in files {
            let lower = path.to_ascii_lowercase();
            if lower.ends_with(".uasset") {
                for alias in package_aliases_for_uasset(path) {
                    uasset_by_alias
                        .entry(alias)
                        .or_default()
                        .insert(path.clone());
                }
            } else if lower.ends_with(".uexp") {
                uexp_by_stem.insert(normalized_stem(path, ".uexp"), path.clone());
            }
        }
        Self {
            uasset_by_alias,
            uexp_by_stem,
        }
    }

    fn resolve_registry_package(&self, package_name: &str) -> Option<&String> {
        let matches = self.uasset_by_alias.get(package_name)?;
        (matches.len() == 1)
            .then(|| matches.iter().next())
            .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogPackageCandidate {
    path: String,
    kinds: BTreeSet<CatalogAssetKind>,
}

fn select_catalog_packages(
    index: &PakAssetIndex,
    registry_candidates: impl IntoIterator<Item = asset_registry::RegistryCandidate>,
) -> Vec<CatalogPackageCandidate> {
    let mut selected = BTreeMap::<String, CatalogPackageCandidate>::new();

    for registry_candidate in registry_candidates {
        let Some(path) = index.resolve_registry_package(&registry_candidate.package_name) else {
            continue;
        };
        if !is_registry_candidate_path(path) {
            continue;
        }
        selected
            .entry(normalized_stem(path, ".uasset"))
            .or_insert_with(|| CatalogPackageCandidate {
                path: path.clone(),
                kinds: BTreeSet::new(),
            })
            .kinds
            .insert(registry_candidate.kind);
    }

    let mut selected = selected.into_values().collect::<Vec<_>>();
    selected.sort_by_key(|candidate| (candidate.path.to_ascii_lowercase(), candidate.path.clone()));
    selected
}

fn is_asset_registry_path(path: &str) -> bool {
    normalize_path(path)
        .rsplit('/')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("AssetRegistry.bin"))
}

fn is_registry_candidate_path(path: &str) -> bool {
    let lower = normalize_path(path).to_ascii_lowercase();
    lower.ends_with(".uasset")
        && !is_obvious_non_item_path(&lower)
        && !is_non_catalog_asset_path(&lower)
}

fn package_aliases_for_uasset(path: &str) -> BTreeSet<String> {
    let stem = normalized_stem(path, ".uasset");
    let segments = stem
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .collect::<Vec<_>>();
    let mut aliases = BTreeSet::new();
    if !segments.is_empty() {
        aliases.insert(format!("/{}", segments.join("/")));
    }

    if let Some(content_index) = segments.iter().position(|segment| *segment == "content") {
        let remainder = &segments[content_index + 1..];
        if !remainder.is_empty() {
            if content_index >= 2 && segments[content_index - 2] == "plugins" {
                aliases.insert(format!(
                    "/{}/{}",
                    segments[content_index - 1],
                    remainder.join("/")
                ));
            } else {
                aliases.insert(format!("/game/{}", remainder.join("/")));
            }
        }
    }
    aliases
}

// Match extracted JSON `Name` semantics: the export object name is catalog identity.
fn catalog_asset_name(base_export: &BaseExport) -> Option<String> {
    let asset_name = render_fname(&base_export.object_name);
    (!asset_name.trim().is_empty()).then_some(asset_name)
}

fn find_tons_in_properties(properties: &[Property]) -> Option<u16> {
    properties.iter().find_map(find_tons_in_property)
}

fn find_bool_property_by_exact_name(properties: &[Property], name: &str) -> Option<bool> {
    properties
        .iter()
        .find_map(|property| find_bool_property(property, name))
}

fn find_chassis_in_properties(properties: &[Property], variant: &str) -> Option<String> {
    properties
        .iter()
        .find_map(|property| find_chassis_in_property(property, variant))
}

fn find_chassis_in_property(property: &Property, variant: &str) -> Option<String> {
    if let Some(value) = property_string(property)
        && let Some(chassis) = scanner::chassis_from_mech_tag_string(&value, variant)
    {
        return Some(chassis);
    }
    match property {
        Property::StructProperty(property) => find_chassis_in_properties(&property.value, variant),
        Property::ArrayProperty(property) => property
            .value
            .iter()
            .find_map(|property| find_chassis_in_property(property, variant)),
        _ => None,
    }
}

fn find_bool_property(property: &Property, name: &str) -> Option<bool> {
    if property.get_name().get_content() == name
        && let Property::BoolProperty(property) = property
    {
        return Some(property.value);
    }
    match property {
        Property::StructProperty(property) => {
            find_bool_property_by_exact_name(&property.value, name)
        }
        Property::ArrayProperty(property) => property
            .value
            .iter()
            .find_map(|property| find_bool_property(property, name)),
        _ => None,
    }
}

fn property_string(property: &Property) -> Option<String> {
    match property {
        Property::NameProperty(property) => Some(render_fname(&property.value)),
        Property::StrProperty(property) => property.value.clone(),
        Property::TextProperty(property) => property
            .culture_invariant_string
            .clone()
            .or_else(|| property.value.clone()),
        _ => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn find_tons_in_property(property: &Property) -> Option<u16> {
    if property.get_name().get_content() == "Tons" {
        return property_number(property);
    }
    match property {
        Property::StructProperty(property) => find_tons_in_properties(&property.value),
        Property::ArrayProperty(property) => property.value.iter().find_map(find_tons_in_property),
        _ => None,
    }
}

fn property_number(property: &Property) -> Option<u16> {
    match property {
        Property::UInt16Property(property) => Some(property.value),
        Property::UInt32Property(property) => u16::try_from(property.value).ok(),
        Property::UInt64Property(property) => u16::try_from(property.value).ok(),
        Property::Int16Property(property) => u16::try_from(property.value).ok(),
        Property::Int64Property(property) => u16::try_from(property.value).ok(),
        Property::Int8Property(property) => u16::try_from(property.value).ok(),
        Property::IntProperty(property) => u16::try_from(property.value).ok(),
        Property::FloatProperty(property) => float_to_u16(property.value.into_inner() as f64),
        Property::DoubleProperty(property) => float_to_u16(property.value.into_inner()),
        _ => None,
    }
}

fn float_to_u16(value: f64) -> Option<u16> {
    if value.fract() == 0.0 && value >= 0.0 && value <= u16::MAX as f64 {
        Some(value as u16)
    } else {
        None
    }
}

struct PakFile {
    path: PathBuf,
    file: BufReader<File>,
    reader: repak::PakReader,
}

impl PakFile {
    fn open(path: &Path) -> Result<Self, ScarabError> {
        let file = File::open(path).map_err(|source| ScarabError::OpenPak {
            path: path.to_path_buf(),
            source,
        })?;
        let mut file = BufReader::new(file);
        let reader = repak::PakBuilder::new()
            .reader(&mut file)
            .map_err(|error| ScarabError::ReadPak {
                path: path.to_path_buf(),
                reason: repak_error_reason(&error),
            })?;

        Ok(Self {
            path: path.to_path_buf(),
            file,
            reader,
        })
    }

    fn list_files(&self) -> Vec<String> {
        self.reader.files()
    }

    fn read_file(&mut self, path: &str, max_bytes: usize) -> Result<Vec<u8>, ScarabError> {
        let mut output = BoundedBuffer::new(max_bytes);
        match self.reader.read_file(path, &mut self.file, &mut output) {
            Ok(()) => Ok(output.into_inner()),
            Err(repak::Error::Io(source)) if source.kind() == io::ErrorKind::FileTooLarge => {
                Err(ScarabError::PakEntryTooLarge {
                    entry: path.to_string(),
                    limit_bytes: max_bytes,
                })
            }
            Err(error) => Err(ScarabError::ReadPakEntry {
                entry: format!("{}:{path}", self.path.display()),
                reason: repak_error_reason(&error),
            }),
        }
    }
}

fn repak_error_reason(error: &repak::Error) -> String {
    match error {
        repak::Error::Compression => "unsupported compression".to_string(),
        repak::Error::Encryption | repak::Error::Encrypted => "encrypted pak".to_string(),
        repak::Error::Oodle => "pak requires Oodle compression support".to_string(),
        repak::Error::Version { .. } | repak::Error::UnsupportedOrEncrypted(_) => {
            format!("unsupported pak version or encryption: {error}")
        }
        _ => error.to_string(),
    }
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    max_bytes: usize,
}

impl BoundedBuffer {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedBuffer {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let remaining = self.max_bytes.saturating_sub(self.bytes.len());
        if buffer.len() > remaining {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "pak entry exceeds configured read limit",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn is_obvious_non_item_path(path: &str) -> bool {
    const PARTS: &[&str] = &[
        "/animations/",
        "/audio/",
        "/effects/",
        "/levels/",
        "/materials/",
        "/meshes/",
        "/models/",
        "/movies/",
        "/sounds/",
        "/textures/",
        "/ui/",
        "/widgets/",
    ];
    PARTS.iter().any(|part| path.contains(part))
}

fn is_non_catalog_asset_path(lower_path: &str) -> bool {
    const EXCLUDED_PATH_PARTS: &[&str] = &[
        "/audio/",
        "/battlearmor/",
        "/bosses/",
        "/broken/",
        "/campaigndata/",
        "/cinematic/",
        "/cinematics/",
        "/clanelemental/",
        "/cutscene/",
        "/cutscenes/",
        "/damaged/",
        "/demos/",
        "/effects/",
        "/fixedequipment/",
        "/missions/",
        "/movies/",
        "/rewards/",
        "/scenario/",
        "/scenarios/",
        "/sounds/",
        "/specialabilities/",
        "/startconditions/",
        "/_startconditions/",
        "/starter/",
        "/tanks/",
        "/tests/",
        "/turrets/",
        "/vehicle/",
        "/vehicles/",
        "/vtols/",
        "/widgets/",
        "/zombie/",
        "/obsolete/",
        "/_obsolete/",
        "/wwiseaudio/",
        "/ui/",
        "/levels/",
        "/textures/",
        "/texture/",
        "/vfx/",
        "/sfx/",
        "/projectiles/",
        "/marketplace/",
        "/model/",
        "/models/",
        "/_models/",
        "/materials/",
        "/material/",
        "/physics/",
        "/particles/",
        "/statuseffects/",
        "/weaponcomponents/",
        "/weaponhardpoints/",
        "/hardpoints/",
        "/animation/",
        "/weaponpallets/",
        "/achievements/",
        "/ai/",
        "/campaign/",
        "/careermode/",
        "/demo/",
        "/tutorial/",
        "/test/",
        "/loadouts_cinematics/",
        "/startingmechs/",
        "/customstartloadouts/",
        "/custommechloadouts/",
        "/functions/",
        "/slots/",
        "/slottypes/",
        "/weapontypeslots/",
    ];
    const EXCLUDED_STEM_SUFFIXES: &[&str] = &[
        "_vehicle",
        "_turret",
        "_vtol",
        "_leopard",
        "_demolisher",
        "_scorpion",
        "_tank",
    ];
    const EXCLUDED_STEM_MARKERS: &[&str] = &[
        "_boss_",
        "_careersalvage",
        "battlearmor",
        "risc_interference",
        "_cutscene_",
        "_starter_",
        "starter_loadout",
        "_reward_",
        "reward_",
        "_zombie",
        "zombie_",
        "loadoutzombie",
        "_damaged",
        "damaged_",
        "loadoutdamaged",
        "_tutorial",
        "tutorial_",
        "_dlc4reward",
        "dlc4_reward",
        "_dlc5_tag",
        "_dlc5_olesko",
        "_doomedfriendly",
        "mechpunch",
        "lrmcarrier",
        "srmcarrier",
        "box_farfire",
        "box_sureshot",
        "narcbeacondata",
        "attachednarcbeacon",
        "weaponcomponent",
        "requirement",
        "dependency",
    ];

    let stem = asset_stem(lower_path);
    contains_any(lower_path, EXCLUDED_PATH_PARTS)
        || EXCLUDED_STEM_SUFFIXES
            .iter()
            .any(|suffix| stem.ends_with(suffix))
        || contains_any(stem, EXCLUDED_STEM_MARKERS)
        || contains_scenario_marker(stem)
        || is_developer_test_stem(stem)
        || lower_path.contains("/_campaignmechs/")
}

fn is_developer_test_stem(stem: &str) -> bool {
    stem.starts_with("test") || stem.ends_with("_test") || stem.contains("_test_")
}

fn contains_scenario_marker(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    bytes.windows(4).enumerate().any(|(index, window)| {
        (index == 0 || bytes[index - 1] == b'_')
            && window[0] == b'a'
            && window[1].is_ascii_digit()
            && window[2] == b'm'
            && window[3].is_ascii_digit()
    })
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn asset_stem(path: &str) -> &str {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    strip_suffix_ignore_ascii_case(file_name, ".uasset")
}

fn normalized_stem(path: &str, suffix: &str) -> String {
    let normalized = normalize_path(path);
    strip_suffix_ignore_ascii_case(&normalized, suffix).to_ascii_lowercase()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn strip_suffix_ignore_ascii_case<'a>(value: &'a str, suffix: &str) -> &'a str {
    let Some(suffix_start) = value.len().checked_sub(suffix.len()) else {
        return value;
    };
    let Some(value_suffix) = value.get(suffix_start..) else {
        return value;
    };
    if !value_suffix.eq_ignore_ascii_case(suffix) {
        return value;
    }
    value.get(..suffix_start).unwrap_or(value)
}
