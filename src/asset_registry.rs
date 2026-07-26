use std::{
    io::{self, Cursor, SeekFrom},
    panic::{AssertUnwindSafe, catch_unwind},
};

use byteorder::{ByteOrder, LE};
use unreal_asset::{
    Import,
    asset::name_map::NameMap,
    containers::{chain::Chain, indexed_map::IndexedMap, shared_resource::SharedResource},
    custom_version::{CustomVersion, CustomVersionTrait},
    engine_version::{self, EngineVersion},
    error::Error,
    exports::class_export::ClassExport,
    object_version::{ObjectVersion, ObjectVersionUE5},
    reader::{
        archive_reader::ArchiveReader,
        archive_trait::{ArchiveTrait, ArchiveType},
        raw_reader::RawReader,
    },
    registry::{AssetRegistryState, objects::asset_data::AssetData},
    types::{PackageIndex, SerializedNameHeader},
    unversioned::Usmap,
};

use crate::{
    scanner::{self, CatalogAssetKind},
    unreal_name::render_fname,
};

const ASSET_REGISTRY_VERSION_GUID: [u8; 16] = [
    0xe7, 0x9e, 0x7f, 0x71, 0x3a, 0x49, 0xb0, 0xe9, 0x32, 0x91, 0xb3, 0x88, 0x07, 0x81, 0x38, 0x1b,
];
const REMOVED_MD5_HASH_VERSION: i32 = 4;
const ADDED_HARD_MANAGE_VERSION: i32 = 5;
const ADDED_COOKED_MD5_HASH_VERSION: i32 = 6;
const ADDED_DEPENDENCY_FLAGS_VERSION: i32 = 7;
const FIXED_TAGS_VERSION: i32 = 8;
const CLASS_PATHS_VERSION: i32 = 15;
const LATEST_SUPPORTED_VERSION: i32 = 16;
const MAX_REGISTRY_COLLECTION_ENTRIES: usize = 1_000_000;
const MAX_FSTRING_CODE_UNITS: i32 = 131_072;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistryCandidate {
    pub(crate) package_name: String,
    pub(crate) kind: CatalogAssetKind,
}

pub(crate) fn parse_registry(bytes: Vec<u8>) -> Result<Vec<RegistryCandidate>, String> {
    validate_registry_structure(&bytes)?;

    let cursor = Cursor::new(bytes);
    let (object_version, object_version_ue5) =
        engine_version::get_object_versions(EngineVersion::VER_UE4_26);
    let raw_reader = RawReader::new(
        Chain::new(cursor, None),
        object_version,
        object_version_ue5,
        false,
        NameMap::new(),
    );
    let mut reader = CheckedRegistryReader::new(raw_reader);
    let registry = catch_unwind(AssertUnwindSafe(|| AssetRegistryState::new(&mut reader)))
        .map_err(|_| "asset registry parser panicked on malformed input".to_string())?
        .map_err(|error| error.to_string())?;

    if registry.assets_data.len() > MAX_REGISTRY_COLLECTION_ENTRIES {
        return Err(format!(
            "asset registry contains {} records; limit is {MAX_REGISTRY_COLLECTION_ENTRIES}",
            registry.assets_data.len()
        ));
    }

    Ok(registry
        .assets_data
        .iter()
        .filter_map(candidate_from_asset_data)
        .collect())
}

fn validate_registry_structure(bytes: &[u8]) -> Result<(), String> {
    let mut reader = RegistrySliceReader::new(bytes);
    let guid = reader.take(ASSET_REGISTRY_VERSION_GUID.len(), "version GUID")?;
    let version = if guid == ASSET_REGISTRY_VERSION_GUID {
        reader.read_i32("version")?
    } else {
        LATEST_SUPPORTED_VERSION
    };
    if !(REMOVED_MD5_HASH_VERSION..=LATEST_SUPPORTED_VERSION).contains(&version) {
        return Err(format!("unsupported asset registry version {version}"));
    }

    if version < FIXED_TAGS_VERSION {
        let name_offset = reader.read_i64("name table offset")?;
        let asset_data_offset = reader.position();
        if name_offset > 0 {
            validate_name_table(bytes, name_offset)?;
        }
        reader.set_position(asset_data_offset, "asset data")?;
    }

    validate_asset_records(&mut reader, version)?;
    if version < ADDED_DEPENDENCY_FLAGS_VERSION {
        validate_legacy_dependencies(&mut reader, version)?;
    } else {
        validate_flagged_dependencies(&mut reader, version)?;
    }
    validate_package_records(&mut reader, version)?;
    Ok(())
}

fn validate_name_table(bytes: &[u8], name_offset: i64) -> Result<(), String> {
    let name_offset = usize::try_from(name_offset)
        .map_err(|_| "asset registry name table offset is negative".to_string())?;
    let mut reader = RegistrySliceReader::at(bytes, name_offset, "name table")?;
    let name_count = reader.read_count("name table", 8)?;
    for _ in 0..name_count {
        reader.skip_fstring("name table entry")?;
        reader.skip(4, "name table hashes")?;
    }
    Ok(())
}

fn validate_asset_records(
    reader: &mut RegistrySliceReader<'_>,
    version: i32,
) -> Result<(), String> {
    let fname_count = if version >= CLASS_PATHS_VERSION { 6 } else { 5 };
    let minimum_record_bytes = fname_count * 8 + 12;
    let asset_count = reader.read_count("asset records", minimum_record_bytes)?;
    for _ in 0..asset_count {
        reader.skip(fname_count * 8, "asset identity fields")?;
        let tag_count = reader.read_count("asset tags", 12)?;
        for _ in 0..tag_count {
            reader.skip(8, "asset tag name")?;
            reader.skip_fstring("asset tag value")?;
        }
        let chunk_count = reader.read_count("asset chunk IDs", 4)?;
        reader.skip_counted(chunk_count, 4, "asset chunk IDs")?;
        reader.skip(4, "asset package flags")?;
    }
    Ok(())
}

fn validate_legacy_dependencies(
    reader: &mut RegistrySliceReader<'_>,
    version: i32,
) -> Result<(), String> {
    let count_fields = if version >= ADDED_HARD_MANAGE_VERSION {
        6
    } else {
        5
    };
    let node_count = reader.read_count("dependency nodes", 1 + count_fields * 4)?;
    for _ in 0..node_count {
        skip_asset_identifier(reader)?;
        let mut dependency_counts = Vec::with_capacity(count_fields);
        for _ in 0..count_fields {
            dependency_counts.push(reader.read_count("dependency references", 4)?);
        }
        for dependency_count in dependency_counts {
            validate_dependency_indices(reader, dependency_count, node_count)?;
        }
    }
    Ok(())
}

fn validate_flagged_dependencies(
    reader: &mut RegistrySliceReader<'_>,
    version: i32,
) -> Result<(), String> {
    let section_size = reader.read_i64("dependency section size")?;
    let section_size = usize::try_from(section_size)
        .map_err(|_| "asset registry dependency section size is negative".to_string())?;
    let section_end = reader
        .position()
        .checked_add(section_size)
        .ok_or_else(|| "asset registry dependency section size overflows".to_string())?;
    if section_end > reader.len() {
        return Err("asset registry dependency section exceeds bounded input".to_string());
    }

    let node_count = reader.read_count("dependency nodes", 17)?;
    if reader.position() > section_end || node_count > (section_end - reader.position()) / 17 {
        return Err("asset registry dependency-node count exceeds its section".to_string());
    }

    // Legacy name-table readers bypass the checked ArchiveReader array methods,
    // so version 7 needs a complete dependency-section preflight. Newer versions
    // use CheckedRegistryReader directly for their dependency arrays.
    if version < FIXED_TAGS_VERSION {
        for _ in 0..node_count {
            skip_asset_identifier(reader)?;
            validate_flagged_dependency_array(reader, node_count, 8)?;
            validate_dependency_array(reader, node_count)?;
            validate_flagged_dependency_array(reader, node_count, 1)?;
            validate_dependency_array(reader, node_count)?;
        }
        if reader.position() > section_end {
            return Err("asset registry dependency data exceeds its section".to_string());
        }
    }
    reader.set_position(section_end, "dependency section end")?;
    Ok(())
}

fn skip_asset_identifier(reader: &mut RegistrySliceReader<'_>) -> Result<(), String> {
    let fields = reader.read_u8("dependency identifier fields")? & 0x0f;
    reader.skip(
        fields.count_ones() as usize * 8,
        "dependency identifier names",
    )
}

fn validate_flagged_dependency_array(
    reader: &mut RegistrySliceReader<'_>,
    node_count: usize,
    flag_set_width: usize,
) -> Result<(), String> {
    let dependency_count = reader.read_count("dependency references", 4)?;
    validate_dependency_indices(reader, dependency_count, node_count)?;
    let flag_bits = flag_set_width
        .checked_mul(dependency_count)
        .ok_or_else(|| "asset registry dependency flag count overflows".to_string())?;
    let flag_words = flag_bits
        .checked_add(31)
        .ok_or_else(|| "asset registry dependency flag count overflows".to_string())?
        / 32;
    if flag_words > MAX_REGISTRY_COLLECTION_ENTRIES {
        return Err("asset registry dependency flag count exceeds limit".to_string());
    }
    reader.skip_counted(flag_words, 4, "dependency flags")
}

fn validate_dependency_array(
    reader: &mut RegistrySliceReader<'_>,
    node_count: usize,
) -> Result<(), String> {
    let dependency_count = reader.read_count("dependency references", 4)?;
    validate_dependency_indices(reader, dependency_count, node_count)
}

fn validate_dependency_indices(
    reader: &mut RegistrySliceReader<'_>,
    dependency_count: usize,
    node_count: usize,
) -> Result<(), String> {
    for _ in 0..dependency_count {
        let index = reader.read_i32("dependency index")?;
        let index = usize::try_from(index)
            .map_err(|_| format!("asset registry dependency index {index} is negative"))?;
        if index >= node_count {
            return Err(format!(
                "asset registry dependency index {index} exceeds {node_count} nodes"
            ));
        }
    }
    Ok(())
}

fn validate_package_records(
    reader: &mut RegistrySliceReader<'_>,
    version: i32,
) -> Result<(), String> {
    let minimum_record_bytes = 8
        + 8
        + 16
        + if version >= ADDED_COOKED_MD5_HASH_VERSION {
            16
        } else {
            0
        };
    let package_count = reader.read_count("package records", minimum_record_bytes)?;

    // Versions using the legacy name-table wrapper bypass the checked array
    // methods, but their package records have a fixed layout at these versions.
    if version < FIXED_TAGS_VERSION {
        reader.skip_counted(package_count, minimum_record_bytes, "package records")?;
    }
    Ok(())
}

struct RegistrySliceReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> RegistrySliceReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn at(bytes: &'a [u8], position: usize, field: &str) -> Result<Self, String> {
        if position > bytes.len() {
            return Err(format!("asset registry {field} is outside bounded input"));
        }
        Ok(Self { bytes, position })
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn position(&self) -> usize {
        self.position
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn set_position(&mut self, position: usize, field: &str) -> Result<(), String> {
        if position > self.bytes.len() {
            return Err(format!("asset registry {field} is outside bounded input"));
        }
        self.position = position;
        Ok(())
    }

    fn take(&mut self, length: usize, field: &str) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| format!("asset registry {field} length overflows"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| format!("asset registry {field} is truncated"))?;
        self.position = end;
        Ok(value)
    }

    fn skip(&mut self, length: usize, field: &str) -> Result<(), String> {
        self.take(length, field).map(|_| ())
    }

    fn skip_counted(&mut self, count: usize, item_bytes: usize, field: &str) -> Result<(), String> {
        let length = count
            .checked_mul(item_bytes)
            .ok_or_else(|| format!("asset registry {field} size overflows"))?;
        self.skip(length, field)
    }

    fn read_u8(&mut self, field: &str) -> Result<u8, String> {
        self.take(1, field)?
            .first()
            .copied()
            .ok_or_else(|| format!("asset registry {field} is truncated"))
    }

    fn read_i32(&mut self, field: &str) -> Result<i32, String> {
        let bytes: [u8; 4] = self
            .take(4, field)?
            .try_into()
            .map_err(|_| format!("asset registry {field} is truncated"))?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn read_i64(&mut self, field: &str) -> Result<i64, String> {
        let bytes: [u8; 8] = self
            .take(8, field)?
            .try_into()
            .map_err(|_| format!("asset registry {field} is truncated"))?;
        Ok(i64::from_le_bytes(bytes))
    }

    fn read_count(&mut self, field: &str, minimum_item_bytes: usize) -> Result<usize, String> {
        let value = self.read_i32(field)?;
        let count = usize::try_from(value)
            .map_err(|_| format!("asset registry {field} count {value} is negative"))?;
        if count > MAX_REGISTRY_COLLECTION_ENTRIES {
            return Err(format!(
                "asset registry {field} count {count} exceeds {MAX_REGISTRY_COLLECTION_ENTRIES}"
            ));
        }
        if minimum_item_bytes > 0 && count > self.remaining() / minimum_item_bytes {
            return Err(format!(
                "asset registry {field} count {count} exceeds its bounded input"
            ));
        }
        Ok(count)
    }

    fn skip_fstring(&mut self, field: &str) -> Result<(), String> {
        let length = self.read_i32(field)?;
        if !(-MAX_FSTRING_CODE_UNITS..=MAX_FSTRING_CODE_UNITS).contains(&length) {
            return Err(format!("asset registry {field} length {length} is invalid"));
        }
        if length == 0 {
            return Ok(());
        }
        let code_units = length
            .checked_abs()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("asset registry {field} length overflows"))?;
        let bytes_per_unit = if length < 0 { 2 } else { 1 };
        self.skip_counted(code_units, bytes_per_unit, field)
    }
}

struct CheckedRegistryReader<R> {
    inner: R,
}

impl<R> CheckedRegistryReader<R> {
    fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R: ArchiveReader> ArchiveTrait for CheckedRegistryReader<R> {
    fn get_archive_type(&self) -> ArchiveType {
        self.inner.get_archive_type()
    }

    fn get_custom_version<T>(&self) -> CustomVersion
    where
        T: CustomVersionTrait + Into<i32>,
    {
        self.inner.get_custom_version::<T>()
    }

    fn has_unversioned_properties(&self) -> bool {
        self.inner.has_unversioned_properties()
    }

    fn use_event_driven_loader(&self) -> bool {
        self.inner.use_event_driven_loader()
    }

    fn position(&mut self) -> u64 {
        self.inner.position()
    }

    fn seek(&mut self, style: SeekFrom) -> io::Result<u64> {
        self.inner.seek(style)
    }

    fn get_name_map(&self) -> SharedResource<NameMap> {
        self.inner.get_name_map()
    }

    fn get_array_struct_type_override(&self) -> &IndexedMap<String, String> {
        self.inner.get_array_struct_type_override()
    }

    fn get_map_key_override(&self) -> &IndexedMap<String, String> {
        self.inner.get_map_key_override()
    }

    fn get_map_value_override(&self) -> &IndexedMap<String, String> {
        self.inner.get_map_value_override()
    }

    fn get_engine_version(&self) -> EngineVersion {
        self.inner.get_engine_version()
    }

    fn get_object_version(&self) -> ObjectVersion {
        self.inner.get_object_version()
    }

    fn get_object_version_ue5(&self) -> ObjectVersionUE5 {
        self.inner.get_object_version_ue5()
    }

    fn get_mappings(&self) -> Option<&Usmap> {
        self.inner.get_mappings()
    }

    fn get_class_export(&self) -> Option<&ClassExport> {
        self.inner.get_class_export()
    }

    fn get_import(&self, index: PackageIndex) -> Option<Import> {
        self.inner.get_import(index)
    }
}

impl<R: ArchiveReader> ArchiveReader for CheckedRegistryReader<R> {
    fn read_array_with_length<T>(
        &mut self,
        length: i32,
        getter: impl Fn(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        let length = checked_collection_length(length)?;
        let remaining = self
            .data_length()?
            .checked_sub(self.position())
            .ok_or_else(|| {
                Error::invalid_file("asset registry reader position exceeds input".to_string())
            })?;
        if length as u64 > remaining {
            return Err(Error::invalid_file(format!(
                "asset registry collection count {length} exceeds {remaining} remaining input bytes"
            )));
        }
        let mut array = Vec::new();
        array.try_reserve_exact(length).map_err(|error| {
            Error::invalid_file(format!(
                "asset registry allocation for {length} entries failed: {error}"
            ))
        })?;
        for _ in 0..length {
            array.push(getter(self)?);
        }
        Ok(array)
    }

    fn read_array<T>(
        &mut self,
        getter: impl Fn(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        let length = self.read_i32::<LE>()?;
        self.read_array_with_length(length, getter)
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        self.inner.read_u8()
    }

    fn read_i8(&mut self) -> io::Result<i8> {
        self.inner.read_i8()
    }

    fn read_u16<T: ByteOrder>(&mut self) -> io::Result<u16> {
        self.inner.read_u16::<T>()
    }

    fn read_i16<T: ByteOrder>(&mut self) -> io::Result<i16> {
        self.inner.read_i16::<T>()
    }

    fn read_u32<T: ByteOrder>(&mut self) -> io::Result<u32> {
        self.inner.read_u32::<T>()
    }

    fn read_i32<T: ByteOrder>(&mut self) -> io::Result<i32> {
        self.inner.read_i32::<T>()
    }

    fn read_u64<T: ByteOrder>(&mut self) -> io::Result<u64> {
        self.inner.read_u64::<T>()
    }

    fn read_i64<T: ByteOrder>(&mut self) -> io::Result<i64> {
        self.inner.read_i64::<T>()
    }

    fn read_f32<T: ByteOrder>(&mut self) -> io::Result<f32> {
        self.inner.read_f32::<T>()
    }

    fn read_f64<T: ByteOrder>(&mut self) -> io::Result<f64> {
        self.inner.read_f64::<T>()
    }

    fn read_fstring(&mut self) -> Result<Option<String>, Error> {
        self.inner.read_fstring()
    }

    fn read_fstring_name_header(
        &mut self,
        serialized_name_header: SerializedNameHeader,
    ) -> Result<Option<String>, Error> {
        self.inner.read_fstring_name_header(serialized_name_header)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.inner.read_exact(buf)
    }

    fn read_bool(&mut self) -> io::Result<bool> {
        self.inner.read_bool()
    }
}

fn checked_collection_length(length: i32) -> Result<usize, Error> {
    let length = usize::try_from(length)
        .map_err(|_| Error::invalid_file("asset registry contains a negative count".to_string()))?;
    if length > MAX_REGISTRY_COLLECTION_ENTRIES {
        return Err(Error::invalid_file(format!(
            "asset registry collection count {length} exceeds {MAX_REGISTRY_COLLECTION_ENTRIES}"
        )));
    }
    Ok(length)
}

fn candidate_from_asset_data(asset: &AssetData) -> Option<RegistryCandidate> {
    let class_name = asset
        .asset_class
        .as_ref()
        .map(|class_name| class_name.get_content())
        .filter(|class_name| !class_name.trim().is_empty())
        .or_else(|| {
            asset.asset_path.as_ref().and_then(|class_path| {
                let asset_name = class_path.asset_name.get_content();
                (!asset_name.trim().is_empty()).then_some(asset_name)
            })
        })?;
    let kind = scanner::catalog_asset_kind_from_class(&normalize_class_name(&class_name))?;

    let package_name = if asset.package_name.get_content().trim().is_empty() {
        render_fname(&asset.object_path)
    } else {
        render_fname(&asset.package_name)
    };
    let package_name = normalize_package_identity(&package_name)?;

    Some(RegistryCandidate { package_name, kind })
}

pub(crate) fn normalize_class_name(value: &str) -> String {
    let trimmed = value.trim().trim_matches(['\'', '"']);
    let unwrapped = trimmed
        .split_once('\'')
        .map(|(_, value)| value.trim_end_matches('\''))
        .unwrap_or(trimmed);
    unwrapped
        .rsplit(['/', '.'])
        .next()
        .unwrap_or(unwrapped)
        .trim()
        .to_string()
}

pub(crate) fn normalize_package_identity(value: &str) -> Option<String> {
    let normalized = value.trim().trim_matches(['\'', '"']).replace('\\', "/");
    let unwrapped = normalized
        .split_once('\'')
        .map(|(_, value)| value.trim_end_matches('\''))
        .unwrap_or(&normalized);
    let without_extension = strip_suffix_ignore_ascii_case(unwrapped, ".uasset");
    let slash = without_extension.rfind('/');
    let without_object = match without_extension.rfind('.') {
        Some(dot) if slash.is_none_or(|slash| dot > slash) => &without_extension[..dot],
        _ => without_extension,
    };
    let segments = without_object
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return None;
    }
    Some(format!("/{}", segments.join("/")).to_ascii_lowercase())
}

fn strip_suffix_ignore_ascii_case<'a>(value: &'a str, suffix: &str) -> &'a str {
    let Some(start) = value.len().checked_sub(suffix.len()) else {
        return value;
    };
    match value.get(start..) {
        Some(value_suffix) if value_suffix.eq_ignore_ascii_case(suffix) => {
            value.get(..start).unwrap_or(value)
        }
        _ => value,
    }
}
