use serde::{Deserialize, Serialize};

use crate::trait_catalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemCategory {
    Weapon,
    Equipment,
    Ammo,
}

impl ItemCategory {
    pub fn as_jj_name(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Equipment => "equipment",
            Self::Ammo => "ammo",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JjItem {
    pub category: ItemCategory,
    pub asset_name: String,
    pub data_asset_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CatalogAssetKind {
    Item,
    Mech,
    Loadout,
    Trait,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JjMech {
    pub chassis: String,
    pub variant: String,
    pub tons: Option<u16>,
    pub is_hero: bool,
}

pub(crate) fn variant_from_asset_name(asset_name: &str) -> Option<String> {
    let variant = asset_name
        .strip_suffix("_MDA")
        .unwrap_or(asset_name)
        .strip_suffix("_PLAYABLE")
        .unwrap_or_else(|| asset_name.strip_suffix("_MDA").unwrap_or(asset_name));
    if variant.trim().is_empty() {
        None
    } else {
        Some(variant.to_string())
    }
}

pub(crate) fn is_jj_addable_mech_variant(variant: &str) -> bool {
    let lower = variant.to_ascii_lowercase();
    !variant.starts_with("A1M4_")
        && !variant.ends_with("_Boss")
        && !variant.ends_with("_DLC8")
        && !variant.ends_with("-TUTORIAL")
        && !lower.contains("_tutorial")
        && !lower.contains("tutorial_")
}

pub(crate) fn chassis_from_asset_path_with_variant(
    path: &str,
    _variant: Option<&str>,
) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let parts = normalized.split('/').collect::<Vec<_>>();
    let mechs_index = parts
        .iter()
        .position(|part| part.eq_ignore_ascii_case("Mechs"))?;
    let mut chassis = parts.get(mechs_index + 1)?;
    if chassis.eq_ignore_ascii_case("CLANS") {
        chassis = parts.get(mechs_index + 2)?;
    }
    if chassis.is_empty()
        || chassis.eq_ignore_ascii_case("_common")
        || chassis.eq_ignore_ascii_case("_customheroes")
    {
        None
    } else {
        Some(normalize_chassis_name(chassis).to_string())
    }
}

fn normalize_chassis_name(chassis: &str) -> &str {
    match chassis {
        "AtlasII" => "Atlas",
        "BlackKnight" | "Blackknight" => "Black Knight",
        "Bullshark" => "Bullshark",
        "DireWolf" => "Dire Wolf",
        "EbonJaguar" | "Ebonjaguar" => "Ebon Jaguar",
        "FireMoth" => "Fire Moth",
        "Hatamotochi" => "Hatamoto-Chi",
        "Jagermech" => "JagerMech",
        "JennerIIC" => "Jenner",
        "KingCrab" | "Kingcrab" => "King Crab",
        "KitFox" => "Kit Fox",
        "MadDog" | "Maddog" => "Mad Dog",
        "Marauderii" => "Marauder",
        "MistLynx" => "Mist Lynx",
        "NightGyr" | "Nightgyr" => "Night Gyr",
        "Phoenixhawk" => "Phoenix Hawk",
        "Roughneck" => "Loader King",
        "ShadowCat" => "Shadow Cat",
        "ShadowHawk" | "ShadowHawkIIC" | "Shadowhawk" => "Shadow Hawk",
        "TimberWolf" => "Timber Wolf",
        "Urbanmech" => "UrbanMech",
        other => other,
    }
}

pub(crate) fn chassis_from_mech_tag_string(value: &str, variant: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(index) = lower[offset..].find("mech.") {
        let start = offset + index;
        let candidate = value[start..]
            .split(|character: char| {
                !(character.is_ascii_alphanumeric()
                    || character == '_'
                    || character == '-'
                    || character == '.')
            })
            .next()
            .unwrap_or_default();
        if let Some(chassis) = chassis_from_mech_tag_token(candidate, variant) {
            return Some(chassis);
        }
        offset = start + "mech.".len();
    }
    None
}

fn chassis_from_mech_tag_token(value: &str, variant: &str) -> Option<String> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() < 4 || !parts[0].eq_ignore_ascii_case("Mech") {
        return None;
    }
    if !mech_tag_variant_matches_asset_variant(variant, parts.last()?) {
        return None;
    }
    let chassis = *parts.get(parts.len() - 2)?;
    if chassis.is_empty() {
        None
    } else {
        Some(normalize_chassis_name(chassis).to_string())
    }
}

fn mech_tag_variant_matches_asset_variant(asset_variant: &str, tag_variant: &str) -> bool {
    if asset_variant.eq_ignore_ascii_case(tag_variant) {
        return true;
    }
    let normalized_asset = normalized_variant_token(asset_variant);
    let normalized_tag = normalized_variant_token(tag_variant);
    normalized_tag.len() >= 5 && normalized_asset.contains(&normalized_tag)
}

fn normalized_variant_token(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

pub(crate) fn is_jj_inventory_asset_name(asset_name: &str) -> bool {
    const NON_INVENTORY_SUFFIXES: &[&str] = &[
        "_Demolisher",
        "_Leopard",
        "_LRMCarrier",
        "_Scorpion",
        "_SRMCarrier",
        "_Tank",
        "_Turret",
        "_Vehicle",
        "_VTOL",
    ];
    !NON_INVENTORY_SUFFIXES
        .iter()
        .any(|suffix| asset_name.ends_with(suffix))
}

pub(crate) fn item_category_from_class(class_name: &str) -> Option<ItemCategory> {
    let lower = class_name.to_ascii_lowercase();
    if lower.contains("ammodataasset") {
        return Some(ItemCategory::Ammo);
    }
    if lower.contains("weapondataasset") || lower.contains("meleeweapondataasset") {
        return Some(ItemCategory::Weapon);
    }
    if lower.contains("mascdataasset")
        || lower.contains("bapdataasset")
        || lower.contains("ecmdataasset")
        || lower.contains("jumpjetdataasset")
        || lower.contains("heatsinkdataasset")
        || lower.contains("targetingcomputerdataasset")
        || lower.contains("structuredataasset")
        || lower.contains("armordataasset")
        || lower.contains("enginedataasset")
        || lower.contains("actuatordataasset")
        || lower.contains("equipmentdataasset")
    {
        return Some(ItemCategory::Equipment);
    }
    None
}

pub(crate) fn catalog_asset_kind_from_class(class_name: &str) -> Option<CatalogAssetKind> {
    let lower = class_name.to_ascii_lowercase();
    if item_category_from_class(&lower).is_some() {
        Some(CatalogAssetKind::Item)
    } else if lower.contains("mwmechdataasset") {
        Some(CatalogAssetKind::Mech)
    } else if lower.contains("mwmechloadoutasset") {
        Some(CatalogAssetKind::Loadout)
    } else if trait_catalog::trait_category_from_class(&lower).is_some() {
        Some(CatalogAssetKind::Trait)
    } else {
        None
    }
}
