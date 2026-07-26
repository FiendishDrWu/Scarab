use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use scarab::{CatalogOutputFormat, JjBuildOptions, build_jj_catalogs};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Generate asset catalogs from MechWarrior 5 and its enabled mods"
)]
struct Cli {
    /// MW5 game directory. Scarab locates the base-game pak and enabled manual/Workshop mods.
    #[arg(long = "mw5-dir")]
    mw5_dir: PathBuf,

    /// Output directory relative to the folder containing scarab.exe.
    #[arg(long)]
    output: PathBuf,

    /// Do not include the base-game pak.
    #[arg(long = "exclude-base-game")]
    exclude_base_game: bool,

    /// Trusted base catalog directory containing item/mech/trait/stock .json.gz files.
    #[arg(long = "catalog-input-dir")]
    catalog_input_dir: Option<PathBuf>,

    /// Do not include any mods.
    #[arg(long = "exclude-mods")]
    exclude_mods: bool,

    /// Excluded mod folder name. May be supplied more than once.
    #[arg(long = "exclude-mod")]
    excluded_mod_folders: Vec<String>,

    /// Catalog output format for item_catalog, mech_catalog, and trait_catalog.
    #[arg(long = "catalog-format", default_value = "json-gz")]
    catalog_format: CliCatalogFormat,

    /// Write catalog_build_report.json.
    #[arg(long = "build-report")]
    build_report: bool,

    /// Allow output into the same directory supplied by --catalog-input-dir, replacing input catalogs.
    #[arg(long = "overwrite-input-catalogs")]
    overwrite_input_catalogs: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliCatalogFormat {
    JsonGz,
    Python,
    Json,
}

fn main() {
    let cli = Cli::parse();
    let output_dir = match output_dir_relative_to_exe(&cli.output) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("scarab failed: {message}");
            std::process::exit(2);
        }
    };

    match build_jj_catalogs(JjBuildOptions {
        mw5_dir: cli.mw5_dir,
        catalog_input_dir: cli.catalog_input_dir,
        output_dir,
        include_base_game: !cli.exclude_base_game,
        include_mods: !cli.exclude_mods,
        excluded_mod_folders: cli.excluded_mod_folders,
        catalog_output_format: match cli.catalog_format {
            CliCatalogFormat::JsonGz => CatalogOutputFormat::JsonGz,
            CliCatalogFormat::Python => CatalogOutputFormat::Python,
            CliCatalogFormat::Json => CatalogOutputFormat::Json,
        },
        build_report: cli.build_report,
        overwrite_input_catalogs: cli.overwrite_input_catalogs,
    }) {
        Ok(report) => {
            println!(
                "wrote {} items to {}",
                report.items_emitted, report.outputs.item_catalog
            );
            println!(
                "wrote {} mechs to {}",
                report.mechs_emitted, report.outputs.mech_catalog
            );
            println!(
                "wrote {} stock templates to {}",
                report.stock_templates_emitted, report.outputs.stock_templates_json_gz
            );
            println!(
                "wrote {} traits to {}",
                report.traits_emitted, report.outputs.trait_catalog
            );
            if let Some(report_path) = report.outputs.catalog_build_report_json {
                println!("report: {report_path}");
            }
        }
        Err(error) => {
            eprintln!("scarab failed: {error}");
            std::process::exit(1);
        }
    }
}

fn output_dir_relative_to_exe(output: &PathBuf) -> Result<PathBuf, String> {
    if output.is_absolute() {
        return Err("--output must be relative to the folder containing scarab.exe".to_string());
    }
    let exe =
        std::env::current_exe().map_err(|error| format!("could not locate scarab.exe: {error}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "could not locate the folder containing scarab.exe".to_string())?;
    Ok(exe_dir.join(output))
}
