//! Scarab generates structured asset catalogs from MechWarrior 5 and its enabled mods.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use flate2::{Compression, GzBuilder, read::GzDecoder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod asset_registry;
mod compatibility;
mod pak;
mod python_export;
mod scanner;
mod stock_template;
mod trait_catalog;
mod unreal_name;

pub use scanner::{ItemCategory, JjItem, JjMech};
pub use stock_template::JjStockTemplate;
pub use trait_catalog::{JjTrait, TraitCategory};

pub const REPORT_SCHEMA_VERSION: u32 = 6;

#[derive(Debug, Error)]
pub enum ScarabError {
    #[error("could not read directory `{path}`: {source}")]
    ReadDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not read file `{path}`: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not parse JSON file `{path}`: {source}")]
    ParseJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("could not decompress gzip file `{path}`: {source}")]
    DecompressGzip {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("missing required catalog input file `{path}`")]
    MissingCatalogInput { path: PathBuf },
    #[error("catalog input directory `{path}` is missing or is not a directory")]
    MissingCatalogInputDirectory { path: PathBuf },
    #[error("catalog input `{path}` has invalid structure: {reason}")]
    InvalidCatalogInput { path: PathBuf, reason: String },
    #[error("could not resolve directory `{path}` for safety check: {source}")]
    ResolveDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "catalog input directory `{input}` and output directory `{output}` resolve to the same location; Scarab will not overwrite input catalogs by default. Select a different --output directory or pass --overwrite-input-catalogs to explicitly authorize replacing the input catalog bundle"
    )]
    CatalogInputOutputConflict { input: PathBuf, output: PathBuf },
    #[error("invalid option combination: {0}")]
    InvalidOptions(String),
    #[error("mod `{mod_name}` is enabled in modlist.json but `{path}` is missing")]
    MissingModJson { mod_name: String, path: PathBuf },
    #[error("mod `{mod_name}` is missing defaultLoadOrder in `{path}`")]
    MissingDefaultLoadOrder { mod_name: String, path: PathBuf },
    #[error("mod compatibility processing failed for `{mod_name}`: {reason}")]
    ModCompatibility { mod_name: String, reason: String },
    #[error("could not find MW5Mercs-WindowsNoEditor.pak under `{path}`")]
    MissingBaseGamePak { path: PathBuf },
    #[error("catalog generation needs at least one source; base game and mods were both excluded")]
    NoCatalogSources,
    #[error("could not open pak `{path}`: {source}")]
    OpenPak {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not read pak `{path}`: {reason}")]
    ReadPak { path: PathBuf, reason: String },
    #[error("could not read pak entry `{entry}`: {reason}")]
    ReadPakEntry { entry: String, reason: String },
    #[error("pak entry `{entry}` exceeded the {limit_bytes}-byte read limit")]
    PakEntryTooLarge { entry: String, limit_bytes: usize },
    #[error("could not parse asset `{path}`: {reason}")]
    ParseAsset { path: String, reason: String },
    #[error("could not create output directory `{path}`: {source}")]
    CreateOutputDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not write file `{path}`: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not serialize stock templates: {source}")]
    SerializeStockTemplates { source: serde_json::Error },
    #[error("could not serialize build report: {source}")]
    SerializeReport { source: serde_json::Error },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildOptions {
    catalog_input_dir: Option<PathBuf>,
    pak_paths: Vec<PathBuf>,
    mods_dir: Option<PathBuf>,
    excluded_mod_folders: Vec<String>,
    output_dir: PathBuf,
    catalog_output_format: CatalogOutputFormat,
    build_report: bool,
    overwrite_input_catalogs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JjBuildOptions {
    pub mw5_dir: PathBuf,
    pub catalog_input_dir: Option<PathBuf>,
    pub output_dir: PathBuf,
    pub include_base_game: bool,
    pub include_mods: bool,
    pub excluded_mod_folders: Vec<String>,
    pub catalog_output_format: CatalogOutputFormat,
    pub build_report: bool,
    pub overwrite_input_catalogs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogOutputFormat {
    JsonGz,
    Python,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogBuildReport {
    pub schema_version: u32,
    pub inputs: Vec<String>,
    pub files_scanned: usize,
    pub items_emitted: usize,
    pub weapons_emitted: usize,
    pub equipment_emitted: usize,
    pub ammo_emitted: usize,
    pub mechs_emitted: usize,
    pub chassis_emitted: usize,
    pub stock_loadouts_scanned: usize,
    pub stock_templates_emitted: usize,
    pub traits_emitted: usize,
    pub pilot_traits_emitted: usize,
    pub mech_traits_emitted: usize,
    pub duplicate_mda_stock_templates: BTreeMap<String, DuplicateStockTemplateReport>,
    pub template_only_stock_templates: Vec<TemplateOnlyStockTemplateReport>,
    pub skipped_mod_compatibility: Vec<SkippedModCompatibilityReport>,
    pub sources: Vec<CatalogSourceReport>,
    pub active_overrides: Vec<CatalogOverrideReport>,
    pub outputs: BuildOutputs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedModCompatibilityReport {
    pub source_id: String,
    pub source_name: String,
    pub folder_name: Option<String>,
    pub processor_id: String,
    pub reason: String,
    pub detected_version: Option<String>,
    pub detected_build_number: Option<String>,
    pub supported_version: String,
    pub supported_build_number: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSourceReport {
    pub source_id: String,
    pub source_name: String,
    pub source_kind: CatalogSourceKind,
    pub load_order: Option<i32>,
    pub folder_name: Option<String>,
    pub files_scanned: usize,
    pub items_scanned: usize,
    pub mechs_scanned: usize,
    pub stock_loadouts_scanned: usize,
    pub stock_templates_emitted: usize,
    pub traits_scanned: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogOverrideReport {
    pub entry_kind: String,
    pub entry_id: String,
    pub selected_source_id: String,
    pub selected_source_name: String,
    pub overridden_source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateOnlyStockTemplateReport {
    pub entry_id: String,
    pub selected_source_id: String,
    pub selected_source_name: String,
    pub source_asset_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSourceKind {
    Direct,
    Mod,
}

#[derive(Debug, Clone, PartialEq)]
struct CatalogSource {
    source_id: String,
    source_name: String,
    source_kind: CatalogSourceKind,
    load_order: Option<i32>,
    folder_name: Option<String>,
    mod_identity: Option<compatibility::ModIdentity>,
    precedence: SourcePrecedence,
    loadout_aliases: BTreeSet<String>,
    scan: ScanAccumulator,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SourcePrecedence {
    layer: u8,
    load_order: i32,
    folder_key: String,
    source_key: String,
    sequence: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct SourceCatalog {
    source_id: String,
    source_name: String,
    source_kind: CatalogSourceKind,
    load_order: Option<i32>,
    folder_name: Option<String>,
    precedence: SourcePrecedence,
    files_scanned: usize,
    items: Vec<SourcedValue<JjItem>>,
    mechs: Vec<SourcedMech>,
    stock_template_types: Vec<SourcedStockTemplateTypes>,
    stock_templates: Vec<SourcedStockTemplate>,
    traits: Vec<SourcedValue<JjTrait>>,
    stock_loadouts_scanned: usize,
    stock_templates_emitted: usize,
    duplicate_mda_stock_templates: BTreeMap<String, DuplicateStockTemplateReport>,
    compatibility_skip: Option<compatibility::CompatibilitySkip>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourcedValue<T> {
    value: T,
    source: SourceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourcedMech {
    value: JjMech,
    source: SourceIdentity,
    presentation: Option<compatibility::MechPresentation>,
    hero_name_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct SourcedStockTemplate {
    key: String,
    template: JjStockTemplate,
    source_asset_name: String,
    source: SourceIdentity,
}

#[derive(Debug, Clone, PartialEq)]
struct SourcedStockTemplateTypes {
    value: stock_template::StockTemplateTypes,
    source: SourceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceIdentity {
    source_id: String,
    source_name: String,
    precedence: SourcePrecedence,
}

#[derive(Debug, Default)]
struct MergeReport {
    active_overrides: Vec<CatalogOverrideReport>,
    template_only_stock_templates: Vec<TemplateOnlyStockTemplateReport>,
}

#[derive(Debug, Deserialize)]
struct Mw5ModList {
    #[serde(rename = "modStatus", default)]
    mod_status: BTreeMap<String, Mw5ModStatus>,
}

#[derive(Debug, Deserialize)]
struct Mw5ModStatus {
    #[serde(rename = "bEnabled", default)]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct Mw5ModMetadata {
    #[serde(rename = "defaultLoadOrder")]
    default_load_order: Option<i32>,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(default)]
    version: Option<serde_json::Value>,
    #[serde(rename = "buildNumber", default)]
    build_number: Option<serde_json::Value>,
    #[serde(rename = "steamPublishedFileId", default)]
    steam_published_file_id: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateStockTemplateReport {
    pub included_in_stock_templates_json_gz: String,
    pub excluded_from_stock_templates_json_gz: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildOutputs {
    pub item_catalog: String,
    pub mech_catalog: String,
    pub stock_templates_json_gz: String,
    pub trait_catalog: String,
    pub catalog_build_report_json: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct ScanAccumulator {
    files_scanned: usize,
    items: Vec<JjItem>,
    mechs: Vec<JjMech>,
    hero_name_overrides: BTreeMap<String, String>,
    stock_template_types: BTreeMap<String, stock_template::StockTemplateTypes>,
    stock_templates: Vec<stock_template::StockTemplateLoadout>,
    traits: Vec<JjTrait>,
}

fn build_catalog_files(options: BuildOptions) -> Result<CatalogBuildReport, ScarabError> {
    validate_catalog_input_output_paths(&options)?;
    let sources = collect_catalog_sources(&options)?;
    let enabled_loadout_tags = enabled_loadout_tags(&sources);
    let source_catalogs = sources
        .into_iter()
        .map(|source| finalize_source_catalog(source, &enabled_loadout_tags))
        .collect::<Result<Vec<_>, _>>()?;
    let source_reports = source_catalogs
        .iter()
        .map(SourceCatalog::report)
        .collect::<Vec<_>>();
    let duplicate_mda_stock_templates = source_catalogs
        .iter()
        .flat_map(|source| {
            source
                .duplicate_mda_stock_templates
                .iter()
                .map(|(key, report)| (format!("{}:{key}", source.source_id), report.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let skipped_mod_compatibility = source_catalogs
        .iter()
        .filter_map(SourceCatalog::skipped_mod_compatibility_report)
        .collect::<Vec<_>>();

    let MergedCatalog {
        items,
        mechs,
        hero_name_overrides,
        stock_templates,
        traits,
        report: merge_report,
    } = merge_source_catalogs(&source_catalogs);

    fs::create_dir_all(&options.output_dir).map_err(|source| {
        ScarabError::CreateOutputDirectory {
            path: options.output_dir.clone(),
            source,
        }
    })?;

    let item_catalog = options.output_dir.join(catalog_output_file_name(
        "item_catalog",
        options.catalog_output_format,
    ));
    write_catalog_output(
        &item_catalog,
        render_item_catalog_json(&items),
        || python_export::render_item_catalog_py(&items),
        options.catalog_output_format,
    )?;

    let mech_catalog = options.output_dir.join(catalog_output_file_name(
        "mech_catalog",
        options.catalog_output_format,
    ));
    write_catalog_output(
        &mech_catalog,
        render_mech_catalog_json_with_hero_names(&mechs, &hero_name_overrides),
        || python_export::render_mech_catalog_py_with_hero_names(&mechs, &hero_name_overrides),
        options.catalog_output_format,
    )?;

    let stock_templates_json_gz = options.output_dir.join("stock_templates.json.gz");
    write_stock_templates_json_gz(&stock_templates_json_gz, &stock_templates)?;

    let trait_catalog = options.output_dir.join(catalog_output_file_name(
        "trait_catalog",
        options.catalog_output_format,
    ));
    write_catalog_output(
        &trait_catalog,
        render_trait_catalog_json(&traits),
        || python_export::render_trait_catalog_py(&traits),
        options.catalog_output_format,
    )?;

    let weapons = items
        .iter()
        .filter(|item| item.category == ItemCategory::Weapon)
        .count();
    let equipment = items
        .iter()
        .filter(|item| item.category == ItemCategory::Equipment)
        .count();
    let ammo = items
        .iter()
        .filter(|item| item.category == ItemCategory::Ammo)
        .count();
    let chassis = mechs
        .iter()
        .map(|mech| mech.chassis.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let pilot_traits = traits
        .iter()
        .filter(|catalog_trait| catalog_trait.category == TraitCategory::Pilot)
        .count();
    let mech_traits = traits
        .iter()
        .filter(|catalog_trait| catalog_trait.category == TraitCategory::Mech)
        .count();
    let stock_loadouts_scanned = source_reports
        .iter()
        .map(|source| source.stock_loadouts_scanned)
        .sum();
    let files_scanned = source_reports
        .iter()
        .map(|source| source.files_scanned)
        .sum();
    let report_path = options.output_dir.join("catalog_build_report.json");
    let report = CatalogBuildReport {
        schema_version: REPORT_SCHEMA_VERSION,
        inputs: report_inputs(&options),
        files_scanned,
        items_emitted: items.len(),
        weapons_emitted: weapons,
        equipment_emitted: equipment,
        ammo_emitted: ammo,
        mechs_emitted: mechs.len(),
        chassis_emitted: chassis,
        stock_loadouts_scanned,
        stock_templates_emitted: stock_templates.len(),
        traits_emitted: traits.len(),
        pilot_traits_emitted: pilot_traits,
        mech_traits_emitted: mech_traits,
        duplicate_mda_stock_templates,
        template_only_stock_templates: merge_report.template_only_stock_templates,
        skipped_mod_compatibility,
        sources: source_reports,
        active_overrides: merge_report.active_overrides,
        outputs: BuildOutputs {
            item_catalog: item_catalog.display().to_string(),
            mech_catalog: mech_catalog.display().to_string(),
            stock_templates_json_gz: stock_templates_json_gz.display().to_string(),
            trait_catalog: trait_catalog.display().to_string(),
            catalog_build_report_json: options
                .build_report
                .then(|| report_path.display().to_string()),
        },
    };
    if options.build_report {
        let report_json = serde_json::to_string_pretty(&report)
            .map_err(|source| ScarabError::SerializeReport { source })?;
        fs::write(&report_path, format!("{report_json}\n")).map_err(|source| {
            ScarabError::WriteFile {
                path: report_path,
                source,
            }
        })?;
    }

    Ok(report)
}

pub fn build_jj_catalogs(options: JjBuildOptions) -> Result<CatalogBuildReport, ScarabError> {
    let mut pak_paths = Vec::new();
    if options.catalog_input_dir.is_some() && !options.include_base_game {
        return Err(ScarabError::InvalidOptions(
            "--catalog-input-dir supplies the base catalog layer and cannot be combined with --exclude-base-game".to_string(),
        ));
    }
    if options.include_base_game && options.catalog_input_dir.is_none() {
        pak_paths.push(find_base_game_pak(&options.mw5_dir)?);
    }
    let mods_dir = if options.include_mods {
        find_mods_folder(&options.mw5_dir)
    } else {
        None
    };
    if options.catalog_input_dir.is_none() && pak_paths.is_empty() && mods_dir.is_none() {
        return Err(ScarabError::NoCatalogSources);
    }
    build_catalog_files(BuildOptions {
        catalog_input_dir: options.catalog_input_dir,
        pak_paths,
        mods_dir,
        excluded_mod_folders: options.excluded_mod_folders,
        output_dir: options.output_dir,
        catalog_output_format: options.catalog_output_format,
        build_report: options.build_report,
        overwrite_input_catalogs: options.overwrite_input_catalogs,
    })
}

fn find_base_game_pak(mw5_dir: &Path) -> Result<PathBuf, ScarabError> {
    const BASE_PAK: &str = "MW5Mercs-WindowsNoEditor.pak";
    let candidates = [
        mw5_dir
            .join("MW5Mercs")
            .join("Content")
            .join("Paks")
            .join(BASE_PAK),
        mw5_dir.join("Content").join("Paks").join(BASE_PAK),
        mw5_dir.join("Paks").join(BASE_PAK),
        mw5_dir.join(BASE_PAK),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| ScarabError::MissingBaseGamePak {
            path: mw5_dir.to_path_buf(),
        })
}

fn find_mods_folder(mw5_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        mw5_dir.join("Mods"),
        mw5_dir.join("MW5Mercs").join("Mods"),
        mw5_dir
            .parent()
            .map(|parent| parent.join("Mods"))
            .unwrap_or_else(|| mw5_dir.join("..").join("Mods")),
    ];
    candidates.into_iter().find(|path| path.is_dir())
}

fn find_steam_workshop_mods_folder(path: &Path) -> Option<PathBuf> {
    const MW5_STEAM_APP_ID: &str = "784080";

    let steamapps_dir = path.ancestors().find(|ancestor| {
        ancestor
            .file_name()
            .map(|name| name.to_string_lossy().eq_ignore_ascii_case("steamapps"))
            .unwrap_or(false)
    })?;
    let workshop_mods_dir = steamapps_dir
        .join("workshop")
        .join("content")
        .join(MW5_STEAM_APP_ID);

    workshop_mods_dir.is_dir().then_some(workshop_mods_dir)
}

struct MergedCatalog {
    items: Vec<JjItem>,
    mechs: Vec<JjMech>,
    hero_name_overrides: BTreeMap<String, String>,
    stock_templates: BTreeMap<String, JjStockTemplate>,
    traits: Vec<JjTrait>,
    report: MergeReport,
}

fn collect_catalog_sources(options: &BuildOptions) -> Result<Vec<CatalogSource>, ScarabError> {
    let mut sources = Vec::new();
    if let Some(catalog_input_dir) = &options.catalog_input_dir {
        sources.push(load_catalog_bundle_source(catalog_input_dir)?);
    }
    if !options.pak_paths.is_empty() {
        let mut scan = ScanAccumulator::default();
        for pak_path in &options.pak_paths {
            pak::scan_pak_file(pak_path, &mut scan)?;
        }
        sources.push(CatalogSource {
            source_id: "direct".to_string(),
            source_name: "direct inputs".to_string(),
            source_kind: CatalogSourceKind::Direct,
            load_order: None,
            folder_name: None,
            mod_identity: None,
            precedence: SourcePrecedence {
                layer: 0,
                load_order: i32::MIN,
                folder_key: String::new(),
                source_key: "direct".to_string(),
                sequence: 0,
            },
            loadout_aliases: BTreeSet::new(),
            scan,
        });
    }
    if let Some(mods_dir) = &options.mods_dir {
        sources.extend(scan_enabled_mod_sources(
            mods_dir,
            sources.len(),
            &options.excluded_mod_folders,
        )?);
    }
    Ok(sources)
}

fn load_catalog_bundle_source(catalog_input_dir: &Path) -> Result<CatalogSource, ScarabError> {
    if !catalog_input_dir.is_dir() {
        return Err(ScarabError::MissingCatalogInputDirectory {
            path: catalog_input_dir.to_path_buf(),
        });
    }
    let mut scan = ScanAccumulator::default();
    let item_path = canonical_catalog_input_file(catalog_input_dir, "item_catalog")?;
    let mech_path = canonical_catalog_input_file(catalog_input_dir, "mech_catalog")?;
    let trait_path = canonical_catalog_input_file(catalog_input_dir, "trait_catalog")?;
    let stock_templates_path =
        required_catalog_input_file(catalog_input_dir, "stock_templates.json.gz")?;

    scan.items = read_item_catalog_file(&item_path)?;
    let mech_catalog = read_mech_catalog_file(&mech_path)?;
    scan.mechs = mech_catalog.mechs;
    scan.hero_name_overrides = mech_catalog.hero_name_overrides;
    scan.traits = read_trait_catalog_file(&trait_path)?;
    for (key, template) in read_stock_template_catalog_file(&stock_templates_path)? {
        scan.stock_template_types.insert(
            key.clone(),
            stock_template::stock_types_from_template(&key, &template),
        );
        scan.stock_templates
            .push(stock_template::StockTemplateLoadout {
                key: key.clone(),
                source_asset_name: key,
                template,
            });
    }
    scan.files_scanned = 4;

    Ok(CatalogSource {
        source_id: "catalog-input".to_string(),
        source_name: catalog_input_dir.display().to_string(),
        source_kind: CatalogSourceKind::Direct,
        load_order: None,
        folder_name: None,
        mod_identity: None,
        precedence: SourcePrecedence {
            layer: 0,
            load_order: i32::MIN,
            folder_key: String::new(),
            source_key: "catalog-input".to_string(),
            sequence: 0,
        },
        loadout_aliases: BTreeSet::new(),
        scan,
    })
}

fn scan_enabled_mod_sources(
    mods_dir: &Path,
    sequence_start: usize,
    excluded_mod_folders: &[String],
) -> Result<Vec<CatalogSource>, ScarabError> {
    let workshop_mods_dir = find_steam_workshop_mods_folder(mods_dir);
    let excluded = excluded_mod_folders
        .iter()
        .map(|folder_name| folder_name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let modlist_path = mods_dir.join("modlist.json");
    let modlist = read_typed_json_file::<Mw5ModList>(&modlist_path)?;
    let mut enabled_mods = modlist
        .mod_status
        .into_iter()
        .filter(|(_, status)| status.enabled)
        .map(|(folder_name, _)| folder_name)
        .collect::<Vec<_>>();
    enabled_mods.retain(|folder_name| !excluded.contains(&folder_name.to_ascii_lowercase()));
    enabled_mods.sort_by_key(|folder_name| folder_name.to_ascii_lowercase());

    let mut sources = Vec::new();
    for (index, folder_name) in enabled_mods.into_iter().enumerate() {
        let mod_folder = std::iter::once(mods_dir)
            .chain(workshop_mods_dir.as_deref())
            .map(|root| root.join(&folder_name))
            .find(|folder| folder.join("mod.json").is_file())
            .unwrap_or_else(|| mods_dir.join(&folder_name));
        let mod_json_path = mod_folder.join("mod.json");
        if !mod_json_path.is_file() {
            return Err(ScarabError::MissingModJson {
                mod_name: folder_name,
                path: mod_json_path,
            });
        }
        let metadata = read_typed_json_file::<Mw5ModMetadata>(&mod_json_path)?;
        let load_order =
            metadata
                .default_load_order
                .ok_or_else(|| ScarabError::MissingDefaultLoadOrder {
                    mod_name: folder_name.clone(),
                    path: mod_json_path.clone(),
                })?;
        let mut scan = ScanAccumulator::default();
        let pak_paths = collect_files_with_extension(&mod_folder, "pak")?;
        for pak_path in &pak_paths {
            pak::scan_pak_file(pak_path, &mut scan)?;
        }
        let source_id = stable_source_id(&folder_name);
        let loadout_aliases =
            loadout_aliases_for_mod(&folder_name, metadata.display_name.as_deref());
        let mod_identity = compatibility::ModIdentity {
            folder_name: folder_name.clone(),
            display_name: metadata.display_name.clone(),
            version: metadata_scalar(metadata.version.as_ref()),
            build_number: metadata_scalar(metadata.build_number.as_ref()),
            steam_published_file_id: metadata_scalar(metadata.steam_published_file_id.as_ref()),
        };
        sources.push(CatalogSource {
            source_id: source_id.clone(),
            source_name: folder_name.clone(),
            source_kind: CatalogSourceKind::Mod,
            load_order: Some(load_order),
            folder_name: Some(folder_name.clone()),
            mod_identity: Some(mod_identity),
            precedence: SourcePrecedence {
                layer: 1,
                load_order,
                folder_key: folder_name.to_ascii_lowercase(),
                source_key: source_id,
                sequence: sequence_start + index,
            },
            loadout_aliases,
            scan,
        });
    }
    sources.sort_by(compare_catalog_sources_for_game_order);
    Ok(sources)
}

fn compare_catalog_sources_for_game_order(left: &CatalogSource, right: &CatalogSource) -> Ordering {
    left.precedence
        .load_order
        .cmp(&right.precedence.load_order)
        .then_with(|| left.precedence.folder_key.cmp(&right.precedence.folder_key))
        .then_with(|| left.source_name.cmp(&right.source_name))
}

fn finalize_source_catalog(
    source: CatalogSource,
    enabled_loadout_tags: &BTreeSet<String>,
) -> Result<SourceCatalog, ScarabError> {
    finalize_source_catalog_with_processors(
        source,
        enabled_loadout_tags,
        compatibility::registered_processors(),
    )
}

fn finalize_source_catalog_with_processors(
    mut source: CatalogSource,
    enabled_loadout_tags: &BTreeSet<String>,
    processors: &[compatibility::ProcessorRegistration],
) -> Result<SourceCatalog, ScarabError> {
    source.scan.items.sort();
    source
        .scan
        .items
        .dedup_by(|left, right| left.asset_name.eq_ignore_ascii_case(&right.asset_name));
    source.scan.mechs.sort();
    source
        .scan
        .mechs
        .dedup_by(|left, right| left.variant.eq_ignore_ascii_case(&right.variant));
    source.scan.traits.sort();
    source.scan.traits.dedup_by(|left, right| {
        left.category == right.category && left.asset_name.eq_ignore_ascii_case(&right.asset_name)
    });

    let (source_compatibility, compatibility_skip) = if let Some(mod_identity) =
        &source.mod_identity
    {
        let view = compatibility::SourceView {
            identity: mod_identity,
            source_id: &source.source_id,
            load_order: source.load_order.unwrap_or(i32::MIN),
            items: &source.scan.items,
            mechs: &source.scan.mechs,
            stock_template_types: &source.scan.stock_template_types,
            stock_templates: &source.scan.stock_templates,
            traits: &source.scan.traits,
        };
        let outcome = compatibility::process_source(&view, processors).map_err(|reason| {
            ScarabError::ModCompatibility {
                mod_name: mod_identity.folder_name.clone(),
                reason,
            }
        })?;
        match outcome {
            compatibility::CompatibilityOutcome::NoMatch => {
                (compatibility::SourceCompatibility::default(), None)
            }
            compatibility::CompatibilityOutcome::Applied(compatibility) => (compatibility, None),
            compatibility::CompatibilityOutcome::SkippedVersionBuild(skip) => {
                (compatibility::SourceCompatibility::default(), Some(skip))
            }
        }
    } else {
        (compatibility::SourceCompatibility::default(), None)
    };

    let identity = SourceIdentity {
        source_id: source.source_id.clone(),
        source_name: source.source_name.clone(),
        precedence: source.precedence.clone(),
    };
    let active_conditional_tags = enabled_loadout_tags
        .difference(&source.loadout_aliases)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut stock_templates = BTreeMap::new();
    let mut loadout_sources_by_key = BTreeMap::<String, Vec<String>>::new();
    let mut included_loadout_by_key = BTreeMap::<String, String>::new();
    let stock_loadouts_scanned = source.scan.stock_templates.len();
    for loadout in source.scan.stock_templates {
        let key = loadout.key;
        let mut template = loadout.template;
        let stock_types = source.scan.stock_template_types.get(&key).or_else(|| {
            source
                .scan
                .stock_template_types
                .values()
                .find(|stock_types| stock_types.key.eq_ignore_ascii_case(&key))
        });
        let output_key = stock_types
            .map(|stock_types| stock_types.key.clone())
            .unwrap_or(key);
        stock_template::reset_stock_types(&mut template);
        if let Some(variant) = output_key.strip_suffix("_MDA") {
            template.variant = variant.to_string();
        }
        loadout_sources_by_key
            .entry(output_key.clone())
            .or_default()
            .push(loadout.source_asset_name.clone());
        let candidate = SourcedStockTemplate {
            key: output_key.clone(),
            template,
            source_asset_name: loadout.source_asset_name,
            source: identity.clone(),
        };
        if should_replace_sourced_stock_template(
            stock_templates.get(&output_key),
            &candidate,
            &active_conditional_tags,
        ) {
            included_loadout_by_key.insert(output_key.clone(), candidate.source_asset_name.clone());
            stock_templates.insert(output_key, candidate);
        }
    }

    let catalog_hero_name_overrides = source.scan.hero_name_overrides;
    Ok(SourceCatalog {
        source_id: source.source_id,
        source_name: source.source_name,
        source_kind: source.source_kind,
        load_order: source.load_order,
        folder_name: source.folder_name,
        precedence: source.precedence.clone(),
        files_scanned: source.scan.files_scanned,
        items: source
            .scan
            .items
            .into_iter()
            .map(|value| SourcedValue {
                value,
                source: identity.clone(),
            })
            .collect(),
        mechs: source
            .scan
            .mechs
            .into_iter()
            .map(|value| {
                let presentation = source_compatibility
                    .presentation_for(&value.variant)
                    .cloned();
                let hero_name_override = presentation
                    .as_ref()
                    .and_then(|presentation| presentation.hero_name.clone())
                    .or_else(|| catalog_hero_name_overrides.get(&value.variant).cloned());
                SourcedMech {
                    presentation,
                    hero_name_override,
                    value,
                    source: identity.clone(),
                }
            })
            .collect(),
        stock_template_types: source
            .scan
            .stock_template_types
            .into_values()
            .map(|value| SourcedStockTemplateTypes {
                value,
                source: identity.clone(),
            })
            .collect(),
        stock_templates: stock_templates.into_values().collect(),
        traits: source
            .scan
            .traits
            .into_iter()
            .map(|value| SourcedValue {
                value,
                source: identity.clone(),
            })
            .collect(),
        stock_loadouts_scanned,
        stock_templates_emitted: included_loadout_by_key.len(),
        duplicate_mda_stock_templates: duplicate_stock_template_report(
            loadout_sources_by_key,
            &included_loadout_by_key,
        ),
        compatibility_skip,
    })
}

fn merge_source_catalogs(sources: &[SourceCatalog]) -> MergedCatalog {
    let mut report = MergeReport::default();
    let items = merge_sourced_values(
        sources
            .iter()
            .flat_map(|source| source.items.iter().cloned()),
        |item| item.asset_name.to_ascii_lowercase(),
        |item| item.asset_name.clone(),
        "item",
        &mut report,
    );
    let (mechs, hero_name_overrides, mech_sources) = merge_sourced_mechs(
        sources
            .iter()
            .flat_map(|source| source.mechs.iter().cloned()),
        &mut report,
    );
    let traits = merge_sourced_values(
        sources
            .iter()
            .flat_map(|source| source.traits.iter().cloned()),
        |catalog_trait| {
            format!(
                "{:?}:{}",
                catalog_trait.category,
                catalog_trait.asset_name.to_ascii_lowercase()
            )
        },
        |catalog_trait| catalog_trait.asset_name.clone(),
        "trait",
        &mut report,
    );
    let stock_templates = merge_stock_templates(sources, &mechs, &mech_sources, &mut report);

    MergedCatalog {
        items,
        mechs,
        hero_name_overrides,
        stock_templates,
        traits,
        report,
    }
}

fn merge_sourced_mechs<I>(
    values: I,
    report: &mut MergeReport,
) -> (
    Vec<JjMech>,
    BTreeMap<String, String>,
    BTreeMap<String, SourceIdentity>,
)
where
    I: IntoIterator<Item = SourcedMech>,
{
    let mut groups = BTreeMap::<String, Vec<SourcedMech>>::new();
    for value in values {
        groups
            .entry(value.value.variant.to_ascii_lowercase())
            .or_default()
            .push(value);
    }

    let mut merged = Vec::new();
    let mut hero_name_overrides = BTreeMap::new();
    let mut mech_sources = BTreeMap::new();
    for (_, mut group) in groups {
        group.sort_by(compare_sourced_mechs);
        let winner = group[0].clone();
        if group.len() > 1 {
            report.active_overrides.push(CatalogOverrideReport {
                entry_kind: "mech".to_string(),
                entry_id: winner.value.variant.clone(),
                selected_source_id: winner.source.source_id.clone(),
                selected_source_name: winner.source.source_name.clone(),
                overridden_source_ids: group
                    .iter()
                    .skip(1)
                    .map(|entry| entry.source.source_id.clone())
                    .collect(),
            });
        }

        let source = winner.source;
        let mut mech = winner.value;
        if let Some(presentation) = winner.presentation {
            mech.chassis = presentation.chassis;
            mech.tons = presentation.tons;
        }
        if let Some(hero_name) = winner.hero_name_override {
            hero_name_overrides.insert(mech.variant.clone(), hero_name);
        }
        mech_sources.insert(mech.variant.to_ascii_lowercase(), source.clone());
        mech_sources.insert(format!("{}_MDA", mech.variant).to_ascii_lowercase(), source);
        merged.push(mech);
    }
    merged.sort();
    (merged, hero_name_overrides, mech_sources)
}

fn merge_sourced_values<T, K, I, F, D>(
    values: I,
    key_for: F,
    display_id_for: D,
    entry_kind: &str,
    report: &mut MergeReport,
) -> Vec<T>
where
    T: Clone + Ord,
    K: Ord,
    I: IntoIterator<Item = SourcedValue<T>>,
    F: Fn(&T) -> K,
    D: Fn(&T) -> String,
{
    let mut groups = BTreeMap::<K, Vec<SourcedValue<T>>>::new();
    for value in values {
        groups.entry(key_for(&value.value)).or_default().push(value);
    }
    let mut merged = Vec::new();
    for (_, mut group) in groups {
        group.sort_by(compare_sourced_values);
        let winner = group[0].clone();
        if group.len() > 1 {
            report.active_overrides.push(CatalogOverrideReport {
                entry_kind: entry_kind.to_string(),
                entry_id: display_id_for(&winner.value),
                selected_source_id: winner.source.source_id.clone(),
                selected_source_name: winner.source.source_name.clone(),
                overridden_source_ids: group
                    .iter()
                    .skip(1)
                    .map(|entry| entry.source.source_id.clone())
                    .collect(),
            });
        }
        merged.push(winner.value);
    }
    merged.sort();
    merged
}

fn merge_stock_templates(
    sources: &[SourceCatalog],
    mechs: &[JjMech],
    mech_sources: &BTreeMap<String, SourceIdentity>,
    report: &mut MergeReport,
) -> BTreeMap<String, JjStockTemplate> {
    // Final template emission depends on the already-merged mech catalog.
    // Loadout and MDA ownership are resolved independently, then recombined.
    let known_mech_keys = mechs
        .iter()
        .flat_map(|mech| {
            [
                mech.variant.to_ascii_lowercase(),
                format!("{}_MDA", mech.variant).to_ascii_lowercase(),
            ]
        })
        .collect::<BTreeSet<_>>();
    let mut stock_types_by_key = BTreeMap::<String, Vec<&SourcedStockTemplateTypes>>::new();
    for stock_types in sources
        .iter()
        .flat_map(|source| source.stock_template_types.iter())
    {
        stock_types_by_key
            .entry(stock_types.value.key.to_ascii_lowercase())
            .or_default()
            .push(stock_types);
    }
    let mut groups = BTreeMap::<String, Vec<SourcedStockTemplate>>::new();
    for template in sources
        .iter()
        .flat_map(|source| source.stock_templates.iter())
    {
        groups
            .entry(template.key.to_ascii_lowercase())
            .or_default()
            .push(template.clone());
    }
    let mut merged = BTreeMap::new();
    for (normalized_key, mut group) in groups {
        group.sort_by(compare_sourced_stock_templates);
        let winner = group[0].clone();
        if group.len() > 1 {
            report.active_overrides.push(CatalogOverrideReport {
                entry_kind: "stock_template".to_string(),
                entry_id: winner.key.clone(),
                selected_source_id: winner.source.source_id.clone(),
                selected_source_name: winner.source.source_name.clone(),
                overridden_source_ids: group
                    .iter()
                    .skip(1)
                    .map(|entry| entry.source.source_id.clone())
                    .collect(),
            });
        }
        if !known_mech_keys.contains(&normalized_key) {
            report
                .template_only_stock_templates
                .push(TemplateOnlyStockTemplateReport {
                    entry_id: winner.key,
                    selected_source_id: winner.source.source_id,
                    selected_source_name: winner.source.source_name,
                    source_asset_name: winner.source_asset_name,
                    reason: "stock template references an MDA that is absent from the final merged mech catalog".to_string(),
                });
            continue;
        }
        let stock_types = mech_sources.get(&normalized_key).and_then(|mda_source| {
            stock_types_by_key.get(&normalized_key).and_then(|values| {
                values
                    .iter()
                    .find(|value| value.source == *mda_source)
                    .map(|value| &value.value)
            })
        });
        let mut output_key = winner.key;
        let mut template = winner.template;
        if let Some(stock_types) = stock_types {
            output_key.clone_from(&stock_types.key);
            stock_template::apply_stock_types(&mut template, stock_types);
            if let Some(variant) = output_key.strip_suffix("_MDA") {
                template.variant = variant.to_string();
            }
        }
        merged.insert(output_key, template);
    }
    merged
}

fn compare_sourced_values<T: Ord>(left: &SourcedValue<T>, right: &SourcedValue<T>) -> Ordering {
    compare_source_identity(&left.source, &right.source).then_with(|| left.value.cmp(&right.value))
}

fn compare_sourced_mechs(left: &SourcedMech, right: &SourcedMech) -> Ordering {
    compare_source_identity(&left.source, &right.source).then_with(|| left.value.cmp(&right.value))
}

fn compare_sourced_stock_templates(
    left: &SourcedStockTemplate,
    right: &SourcedStockTemplate,
) -> Ordering {
    compare_source_identity(&left.source, &right.source)
        .then_with(|| left.source_asset_name.cmp(&right.source_asset_name))
}

fn compare_source_identity(left: &SourceIdentity, right: &SourceIdentity) -> Ordering {
    right
        .precedence
        .cmp(&left.precedence)
        .then_with(|| left.source_id.cmp(&right.source_id))
}

impl SourceCatalog {
    fn report(&self) -> CatalogSourceReport {
        CatalogSourceReport {
            source_id: self.source_id.clone(),
            source_name: self.source_name.clone(),
            source_kind: self.source_kind,
            load_order: self.load_order,
            folder_name: self.folder_name.clone(),
            files_scanned: self.files_scanned,
            items_scanned: self.items.len(),
            mechs_scanned: self.mechs.len(),
            stock_loadouts_scanned: self.stock_loadouts_scanned,
            stock_templates_emitted: self.stock_templates_emitted,
            traits_scanned: self.traits.len(),
        }
    }

    fn skipped_mod_compatibility_report(&self) -> Option<SkippedModCompatibilityReport> {
        let skip = self.compatibility_skip.as_ref()?;
        Some(SkippedModCompatibilityReport {
            source_id: self.source_id.clone(),
            source_name: self.source_name.clone(),
            folder_name: self.folder_name.clone(),
            processor_id: skip.processor_id.clone(),
            reason: "version_or_build_mismatch".to_string(),
            detected_version: skip.detected_version.clone(),
            detected_build_number: skip.detected_build_number.clone(),
            supported_version: skip.supported_version.clone(),
            supported_build_number: skip.supported_build_number.clone(),
            note: "Compatibility instructions were not used because the detected mod version/build does not match this processor; the mod was scanned and merged normally."
                .to_string(),
        })
    }
}

fn collect_files_with_extension(root: &Path, extension: &str) -> Result<Vec<PathBuf>, ScarabError> {
    let mut paths = Vec::new();
    collect_files_with_extension_inner(root, extension, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_files_with_extension_inner(
    path: &Path,
    extension: &str,
    paths: &mut Vec<PathBuf>,
) -> Result<(), ScarabError> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_file() {
        if path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(extension))
            .unwrap_or(false)
        {
            paths.push(path.to_path_buf());
        }
        return Ok(());
    }
    let mut entries = fs::read_dir(path)
        .map_err(|source| ScarabError::ReadDirectory {
            path: path.to_path_buf(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| ScarabError::ReadDirectory {
            path: path.to_path_buf(),
            source,
        })?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        collect_files_with_extension_inner(&entry.path(), extension, paths)?;
    }
    Ok(())
}

fn read_typed_json_file<T>(path: &Path) -> Result<T, ScarabError>
where
    T: for<'de> Deserialize<'de>,
{
    let text = fs::read_to_string(path).map_err(|source| ScarabError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|source| {
        ScarabError::ParseJson {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn read_typed_json_or_gz_file<T>(path: &Path) -> Result<T, ScarabError>
where
    T: for<'de> Deserialize<'de>,
{
    let text = if path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.ends_with(".json.gz"))
    {
        read_gzip_text(path)?
    } else {
        fs::read_to_string(path).map_err(|source| ScarabError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?
    };
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|source| {
        ScarabError::ParseJson {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn read_gzip_text(path: &Path) -> Result<String, ScarabError> {
    let bytes = fs::read(path).map_err(|source| ScarabError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut decoder = GzDecoder::new(bytes.as_slice());
    let mut text = String::new();
    decoder
        .read_to_string(&mut text)
        .map_err(|source| ScarabError::DecompressGzip {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(text)
}

#[derive(Debug, Deserialize)]
struct ItemCatalogJson {
    #[serde(default)]
    weapons: Vec<JjItem>,
    #[serde(default)]
    equipment: Vec<JjItem>,
    #[serde(default)]
    ammo: Vec<JjItem>,
}

#[derive(Debug, Deserialize)]
struct MechCatalogJson {
    chassis: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    hero_names: BTreeMap<String, String>,
    #[serde(default)]
    chassis_tonnage: BTreeMap<String, u16>,
}

#[derive(Debug)]
struct LoadedMechCatalog {
    mechs: Vec<JjMech>,
    hero_name_overrides: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct TraitCatalogJson {
    #[serde(default)]
    pilot_traits: Vec<TraitCatalogRow>,
    #[serde(default)]
    mech_traits: Vec<TraitCatalogRow>,
}

#[derive(Debug, Deserialize)]
struct TraitCatalogRow {
    asset_name: String,
    friendly_label: String,
}

fn read_item_catalog_file(path: &Path) -> Result<Vec<JjItem>, ScarabError> {
    let catalog = read_typed_json_or_gz_file::<ItemCatalogJson>(path)?;
    let mut items = Vec::new();
    for item in catalog
        .weapons
        .into_iter()
        .chain(catalog.equipment)
        .chain(catalog.ammo)
    {
        if item.asset_name.trim().is_empty() {
            return Err(ScarabError::InvalidCatalogInput {
                path: path.to_path_buf(),
                reason: "item asset_name must not be empty".to_string(),
            });
        }
        if item.data_asset_type.trim().is_empty() {
            return Err(ScarabError::InvalidCatalogInput {
                path: path.to_path_buf(),
                reason: format!("item `{}` has an empty data_asset_type", item.asset_name),
            });
        }
        items.push(item);
    }
    Ok(items)
}

fn read_mech_catalog_file(path: &Path) -> Result<LoadedMechCatalog, ScarabError> {
    let catalog = read_typed_json_or_gz_file::<MechCatalogJson>(path)?;
    let mut mechs = Vec::new();
    for (chassis, variants) in catalog.chassis {
        if chassis.trim().is_empty() {
            return Err(ScarabError::InvalidCatalogInput {
                path: path.to_path_buf(),
                reason: "chassis name must not be empty".to_string(),
            });
        }
        for variant in variants {
            if variant.trim().is_empty() {
                return Err(ScarabError::InvalidCatalogInput {
                    path: path.to_path_buf(),
                    reason: format!("chassis `{chassis}` contains an empty variant"),
                });
            }
            mechs.push(JjMech {
                tons: catalog.chassis_tonnage.get(&chassis).copied(),
                is_hero: catalog.hero_names.contains_key(&variant),
                chassis: chassis.clone(),
                variant,
            });
        }
    }
    let variants = mechs
        .iter()
        .map(|mech| mech.variant.as_str())
        .collect::<BTreeSet<_>>();
    for (variant, hero_name) in &catalog.hero_names {
        if !variants.contains(variant.as_str()) {
            return Err(ScarabError::InvalidCatalogInput {
                path: path.to_path_buf(),
                reason: format!("hero_names key `{variant}` is absent from the mech catalog"),
            });
        }
        if hero_name.is_empty()
            || hero_name.trim() != hero_name
            || hero_name.chars().any(char::is_control)
        {
            return Err(ScarabError::InvalidCatalogInput {
                path: path.to_path_buf(),
                reason: format!(
                    "hero_names value for `{variant}` must be nonempty, trimmed, and contain no control characters"
                ),
            });
        }
    }
    Ok(LoadedMechCatalog {
        mechs,
        hero_name_overrides: catalog.hero_names,
    })
}

fn read_trait_catalog_file(path: &Path) -> Result<Vec<JjTrait>, ScarabError> {
    let catalog = read_typed_json_or_gz_file::<TraitCatalogJson>(path)?;
    let mut traits = Vec::new();
    for row in catalog.pilot_traits {
        traits.push(trait_row(path, TraitCategory::Pilot, row)?);
    }
    for row in catalog.mech_traits {
        traits.push(trait_row(path, TraitCategory::Mech, row)?);
    }
    Ok(traits)
}

fn trait_row(
    path: &Path,
    category: TraitCategory,
    row: TraitCatalogRow,
) -> Result<JjTrait, ScarabError> {
    if row.asset_name.trim().is_empty() {
        return Err(ScarabError::InvalidCatalogInput {
            path: path.to_path_buf(),
            reason: "trait asset_name must not be empty".to_string(),
        });
    }
    if row.friendly_label.trim().is_empty() {
        return Err(ScarabError::InvalidCatalogInput {
            path: path.to_path_buf(),
            reason: format!("trait `{}` has an empty friendly_label", row.asset_name),
        });
    }
    Ok(JjTrait {
        category,
        asset_name: row.asset_name,
        friendly_label: row.friendly_label,
    })
}

fn read_stock_template_catalog_file(
    path: &Path,
) -> Result<BTreeMap<String, JjStockTemplate>, ScarabError> {
    let stock_templates = read_typed_json_or_gz_file::<BTreeMap<String, JjStockTemplate>>(path)?;
    for (key, template) in &stock_templates {
        if key.trim().is_empty() {
            return Err(ScarabError::InvalidCatalogInput {
                path: path.to_path_buf(),
                reason: "stock template key must not be empty".to_string(),
            });
        }
        if template.variant.trim().is_empty() {
            return Err(ScarabError::InvalidCatalogInput {
                path: path.to_path_buf(),
                reason: format!("stock template `{key}` has an empty variant"),
            });
        }
    }
    Ok(stock_templates)
}

fn canonical_catalog_input_file(
    catalog_input_dir: &Path,
    stem: &str,
) -> Result<PathBuf, ScarabError> {
    let compressed = catalog_input_dir.join(format!("{stem}.json.gz"));
    if compressed.is_file() {
        return Ok(compressed);
    }
    required_catalog_input_file(catalog_input_dir, &format!("{stem}.json"))
}

fn required_catalog_input_file(
    catalog_input_dir: &Path,
    file_name: &str,
) -> Result<PathBuf, ScarabError> {
    let path = catalog_input_dir.join(file_name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(ScarabError::MissingCatalogInput { path })
    }
}

fn stable_source_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn metadata_scalar(value: Option<&serde_json::Value>) -> Option<String> {
    match value {
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        Some(serde_json::Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn report_inputs(options: &BuildOptions) -> Vec<String> {
    options
        .catalog_input_dir
        .iter()
        .chain(options.pak_paths.iter())
        .chain(options.mods_dir.iter())
        .map(|path| path.display().to_string())
        .collect()
}

fn should_replace_sourced_stock_template(
    existing: Option<&SourcedStockTemplate>,
    candidate: &SourcedStockTemplate,
    active_conditional_tags: &BTreeSet<String>,
) -> bool {
    let Some(existing) = existing else {
        return true;
    };
    stock_template_selection_score(candidate, active_conditional_tags)
        > stock_template_selection_score(existing, active_conditional_tags)
}

fn stock_template_selection_score(
    candidate: &SourcedStockTemplate,
    active_conditional_tags: &BTreeSet<String>,
) -> StockTemplateSelectionScore {
    let matching_tags =
        matching_loadout_tags(&candidate.source_asset_name, active_conditional_tags);
    let longest_matching_tag = matching_tags.iter().map(String::len).max().unwrap_or(0);
    StockTemplateSelectionScore {
        conditional_tag_count: matching_tags.len(),
        longest_matching_tag,
        is_canonical_default: is_canonical_default_loadout(candidate),
        reverse_source_asset_name: std::cmp::Reverse(candidate.source_asset_name.clone()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StockTemplateSelectionScore {
    conditional_tag_count: usize,
    longest_matching_tag: usize,
    is_canonical_default: bool,
    reverse_source_asset_name: std::cmp::Reverse<String>,
}

fn matching_loadout_tags(
    source_asset_name: &str,
    active_conditional_tags: &BTreeSet<String>,
) -> Vec<String> {
    let normalized_name = normalize_loadout_tag(source_asset_name);
    active_conditional_tags
        .iter()
        .filter(|tag| tag.len() >= 3 && normalized_name.contains(tag.as_str()))
        .cloned()
        .collect()
}

fn is_canonical_default_loadout(candidate: &SourcedStockTemplate) -> bool {
    let canonical_source = format!("{}_Loadout", candidate.template.variant);
    candidate
        .source_asset_name
        .eq_ignore_ascii_case(&canonical_source)
}

fn enabled_loadout_tags(sources: &[CatalogSource]) -> BTreeSet<String> {
    sources
        .iter()
        .flat_map(|source| source.loadout_aliases.iter().cloned())
        .collect()
}

fn loadout_aliases_for_mod(folder_name: &str, display_name: Option<&str>) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    add_loadout_aliases_from_text(&mut aliases, folder_name);
    if let Some(display_name) = display_name {
        add_loadout_aliases_from_text(&mut aliases, display_name);
    }
    let compact_folder = normalize_loadout_tag(folder_name);
    let compact_display = display_name.map(normalize_loadout_tag).unwrap_or_default();
    if compact_folder.contains("yetanothermechlab") || compact_display.contains("yetanothermechlab")
    {
        aliases.insert("yaml".to_string());
    }
    if compact_folder.contains("yetanotherweapon") || compact_display.contains("yetanotherweapon") {
        aliases.insert("yaw".to_string());
        if compact_folder.contains("completeedition") || compact_display.contains("completeedition")
        {
            aliases.insert("yawce".to_string());
        }
    }
    aliases.retain(|alias| alias.len() >= 3);
    aliases
}

fn add_loadout_aliases_from_text(aliases: &mut BTreeSet<String>, text: &str) {
    let compact = normalize_loadout_tag(text);
    if !compact.is_empty() {
        aliases.insert(compact);
    }
    let words = split_alias_words(text);
    if words.len() > 1 {
        let acronym = words
            .iter()
            .filter_map(|word| word.chars().next())
            .collect::<String>();
        if acronym.len() >= 3 {
            aliases.insert(acronym);
        }
    }
}

fn split_alias_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_was_lowercase = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            let is_uppercase = character.is_ascii_uppercase();
            if is_uppercase && previous_was_lowercase && !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            current.push(character.to_ascii_lowercase());
            previous_was_lowercase = character.is_ascii_lowercase() || character.is_ascii_digit();
        } else {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            previous_was_lowercase = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn normalize_loadout_tag(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn duplicate_stock_template_report(
    loadout_sources_by_key: BTreeMap<String, Vec<String>>,
    included_loadout_by_key: &BTreeMap<String, String>,
) -> BTreeMap<String, DuplicateStockTemplateReport> {
    loadout_sources_by_key
        .into_iter()
        .filter_map(|(key, loadouts)| {
            if loadouts.len() <= 1 {
                return None;
            }
            let included = included_loadout_by_key
                .get(&key)
                .cloned()
                .unwrap_or_else(|| loadouts[0].clone());
            let excluded = loadouts
                .into_iter()
                .filter(|loadout| loadout != &included)
                .collect();
            Some((
                key,
                DuplicateStockTemplateReport {
                    included_in_stock_templates_json_gz: included,
                    excluded_from_stock_templates_json_gz: excluded,
                },
            ))
        })
        .collect()
}

fn validate_catalog_input_output_paths(options: &BuildOptions) -> Result<(), ScarabError> {
    let Some(catalog_input_dir) = &options.catalog_input_dir else {
        return Ok(());
    };
    let input_identity = directory_identity(catalog_input_dir, true)?;
    let output_identity = directory_identity(&options.output_dir, false)?;
    if input_identity == output_identity && !options.overwrite_input_catalogs {
        return Err(ScarabError::CatalogInputOutputConflict {
            input: catalog_input_dir.clone(),
            output: options.output_dir.clone(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirectoryIdentity(String);

fn directory_identity(path: &Path, must_exist: bool) -> Result<DirectoryIdentity, ScarabError> {
    let resolved = resolve_directory_path(path, must_exist)?;
    let text = resolved.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    let text = text.to_ascii_lowercase();
    Ok(DirectoryIdentity(text.trim_end_matches('/').to_string()))
}

fn resolve_directory_path(path: &Path, must_exist: bool) -> Result<PathBuf, ScarabError> {
    if path.is_dir() {
        return fs::canonicalize(path).map_err(|source| ScarabError::ResolveDirectory {
            path: path.to_path_buf(),
            source,
        });
    }
    if must_exist {
        return Err(ScarabError::MissingCatalogInputDirectory {
            path: path.to_path_buf(),
        });
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| ScarabError::ResolveDirectory {
                path: path.to_path_buf(),
                source,
            })?
            .join(path)
    };
    let mut missing_components = Vec::new();
    let mut candidate = absolute.as_path();
    while !candidate.exists() {
        let Some(name) = candidate.file_name() else {
            break;
        };
        missing_components.push(name.to_os_string());
        candidate = candidate
            .parent()
            .ok_or_else(|| ScarabError::ResolveDirectory {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "could not find an existing parent directory",
                ),
            })?;
    }
    if !candidate.is_dir() {
        return Err(ScarabError::ResolveDirectory {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "existing path component is not a directory",
            ),
        });
    }
    let mut resolved =
        fs::canonicalize(candidate).map_err(|source| ScarabError::ResolveDirectory {
            path: path.to_path_buf(),
            source,
        })?;
    for component in missing_components.into_iter().rev() {
        resolved.push(component);
    }
    Ok(normalize_path_components(&resolved))
}

fn normalize_path_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn write_catalog_output<F>(
    path: &Path,
    json: String,
    python: F,
    format: CatalogOutputFormat,
) -> Result<(), ScarabError>
where
    F: FnOnce() -> String,
{
    match format {
        CatalogOutputFormat::JsonGz => write_gzip_bytes(path, json.as_bytes()),
        CatalogOutputFormat::Json => {
            fs::write(path, json).map_err(|source| ScarabError::WriteFile {
                path: path.to_path_buf(),
                source,
            })
        }
        CatalogOutputFormat::Python => {
            fs::write(path, python()).map_err(|source| ScarabError::WriteFile {
                path: path.to_path_buf(),
                source,
            })
        }
    }
}

fn write_stock_templates_json_gz(
    path: &Path,
    stock_templates: &BTreeMap<String, JjStockTemplate>,
) -> Result<(), ScarabError> {
    let json = serde_json::to_vec(stock_templates)
        .map_err(|source| ScarabError::SerializeStockTemplates { source })?;
    write_gzip_bytes(path, &json)
}

fn write_gzip_bytes(path: &Path, bytes: &[u8]) -> Result<(), ScarabError> {
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .map_err(|source| ScarabError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;
    let compressed = encoder.finish().map_err(|source| ScarabError::WriteFile {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, compressed).map_err(|source| ScarabError::WriteFile {
        path: path.to_path_buf(),
        source,
    })
}

fn catalog_output_file_name(stem: &str, format: CatalogOutputFormat) -> String {
    match format {
        CatalogOutputFormat::JsonGz => format!("{stem}.json.gz"),
        CatalogOutputFormat::Python => format!("{stem}.py"),
        CatalogOutputFormat::Json => format!("{stem}.json"),
    }
}

fn render_item_catalog_json(items: &[JjItem]) -> String {
    python_export::render_item_catalog_json(items)
}

fn render_mech_catalog_json_with_hero_names(
    mechs: &[JjMech],
    hero_name_overrides: &BTreeMap<String, String>,
) -> String {
    python_export::render_mech_catalog_json_with_hero_names(mechs, hero_name_overrides)
}

fn render_trait_catalog_json(traits: &[JjTrait]) -> String {
    python_export::render_trait_catalog_json(traits)
}
