use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use unreal_asset::properties::{Property, PropertyDataTrait};

use crate::{scanner, unreal_name::render_fname};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JjStockTemplate {
    pub variant: String,
    pub armor: BTreeMap<String, f64>,
    #[serde(rename = "maxArmor", default)]
    pub max_armor: BTreeMap<String, f64>,
    #[serde(rename = "rearArmor")]
    pub rear_armor: BTreeMap<String, f64>,
    pub structure: BTreeMap<String, f64>,
    pub weapons: Vec<StockWeapon>,
    pub groups: Vec<StockWeaponGroup>,
    pub equipment: Vec<StockEquipmentPart>,
    #[serde(rename = "armorType")]
    pub armor_type: String,
    #[serde(rename = "structureType")]
    pub structure_type: String,
    #[serde(rename = "armorDataAssetId")]
    pub armor_data_asset_id: PrimaryAssetIdWrapper,
    #[serde(rename = "structureDataAssetId")]
    pub structure_data_asset_id: PrimaryAssetIdWrapper,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockWeapon {
    pub slot: String,
    #[serde(rename = "type")]
    pub asset_type: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockWeaponGroup {
    pub slot: String,
    pub g: [bool; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockEquipmentPart {
    pub part: String,
    pub items: Vec<StockEquipmentItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockEquipmentItem {
    #[serde(rename = "type")]
    pub asset_type: String,
    pub name: String,
    #[serde(rename = "slotId")]
    pub slot_id: i32,
    #[serde(rename = "slotType")]
    pub slot_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryAssetIdWrapper {
    #[serde(rename = "Id")]
    pub id: PrimaryAssetId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryAssetId {
    #[serde(rename = "PrimaryAssetType")]
    pub primary_asset_type: PrimaryAssetName,
    #[serde(rename = "PrimaryAssetName")]
    pub primary_asset_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryAssetName {
    #[serde(rename = "Name")]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StockTemplateTypes {
    pub key: String,
    pub max_armor: BTreeMap<String, f64>,
    pub armor_type: String,
    pub structure_type: String,
    pub armor_data_asset_id: PrimaryAssetIdWrapper,
    pub structure_data_asset_id: PrimaryAssetIdWrapper,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StockTemplateLoadout {
    pub key: String,
    pub source_asset_name: String,
    pub template: JjStockTemplate,
}

pub(crate) fn stock_types_from_template(
    key: &str,
    template: &JjStockTemplate,
) -> StockTemplateTypes {
    StockTemplateTypes {
        key: key.to_string(),
        max_armor: template.max_armor.clone(),
        armor_type: template.armor_type.clone(),
        structure_type: template.structure_type.clone(),
        armor_data_asset_id: template.armor_data_asset_id.clone(),
        structure_data_asset_id: template.structure_data_asset_id.clone(),
    }
}

pub(crate) fn reset_stock_types(template: &mut JjStockTemplate) {
    template.max_armor.clear();
    template.armor_type = "StandardArmor".to_string();
    template.structure_type = "StandardStructure".to_string();
    template.armor_data_asset_id =
        primary_asset_id_wrapper_empty("MWArmorDataAsset", "StandardArmor");
    template.structure_data_asset_id =
        primary_asset_id_wrapper_empty("MWStructureDataAsset", "StandardStructure");
}

pub(crate) fn stock_types_from_mda(
    asset_name: &str,
    properties: &[Property],
) -> Option<StockTemplateTypes> {
    let variant = scanner::variant_from_asset_name(asset_name)?;
    if !scanner::is_jj_addable_mech_variant(&variant) {
        return None;
    }
    let key = asset_name.to_string();
    let mech_data = struct_child(properties, "MechData")?;
    let max_armor = struct_child(mech_data, "HealthDataStats")
        .and_then(|health_data_stats| struct_child(health_data_stats, "MaxArmor"))
        .map(health_map)
        .unwrap_or_default();
    let equipment_allocation = struct_child(mech_data, "EquipmentAllocation")?;
    let armor_data_asset_id =
        primary_asset_id_wrapper(equipment_allocation, "ArmorDataAssetId", "MWArmorDataAsset")?;
    let structure_data_asset_id = primary_asset_id_wrapper(
        equipment_allocation,
        "StructureDataAssetId",
        "MWStructureDataAsset",
    )?;

    Some(StockTemplateTypes {
        key,
        max_armor,
        armor_type: armor_data_asset_id.id.primary_asset_name.clone(),
        structure_type: structure_data_asset_id.id.primary_asset_name.clone(),
        armor_data_asset_id,
        structure_data_asset_id,
    })
}

pub(crate) fn stock_template_from_loadout(
    asset_name: &str,
    properties: &[Property],
) -> Option<StockTemplateLoadout> {
    let loadout = struct_child(properties, "MechLoadout")?;
    let key = primary_asset_name(loadout, &["MechDataAssetId", "Id"])?;
    let variant = scanner::variant_from_asset_name(&key)?;
    if !scanner::is_jj_addable_mech_variant(&variant) {
        return None;
    }

    let armor = health_map(struct_child(loadout, "CurrentArmor")?);
    let rear_armor = health_map(struct_child(loadout, "CurrentRearArmor")?);
    let structure = health_map(struct_child(loadout, "CurrentStructure")?);
    let weapons = weapons(loadout);
    let groups = weapon_groups(loadout);
    let equipment = equipment(loadout);
    let armor_data_asset_id = primary_asset_id_wrapper_empty("MWArmorDataAsset", "StandardArmor");
    let structure_data_asset_id =
        primary_asset_id_wrapper_empty("MWStructureDataAsset", "StandardStructure");

    Some(StockTemplateLoadout {
        key,
        source_asset_name: asset_name.to_string(),
        template: JjStockTemplate {
            variant,
            armor,
            max_armor: BTreeMap::new(),
            rear_armor,
            structure,
            weapons,
            groups,
            equipment,
            armor_type: "StandardArmor".to_string(),
            structure_type: "StandardStructure".to_string(),
            armor_data_asset_id,
            structure_data_asset_id,
        },
    })
}

pub(crate) fn apply_stock_types(template: &mut JjStockTemplate, stock_types: &StockTemplateTypes) {
    template.max_armor.clone_from(&stock_types.max_armor);
    template.armor_type.clone_from(&stock_types.armor_type);
    template
        .structure_type
        .clone_from(&stock_types.structure_type);
    template
        .armor_data_asset_id
        .clone_from(&stock_types.armor_data_asset_id);
    template
        .structure_data_asset_id
        .clone_from(&stock_types.structure_data_asset_id);
}

fn weapons(loadout: &[Property]) -> Vec<StockWeapon> {
    array_child(loadout, "InstalledWeapons")
        .into_iter()
        .flat_map(|values| values.iter())
        .filter_map(|entry| {
            let entry = property_struct(entry)?;
            let slot = name_child(entry, "HardpointSlotID")?;
            let (asset_type, name) = primary_asset_pair(entry, &["WeaponData", "WeaponId", "Id"])?;
            Some(StockWeapon {
                slot,
                asset_type,
                name,
            })
        })
        .collect()
}

fn weapon_groups(loadout: &[Property]) -> Vec<StockWeaponGroup> {
    let Some(group_info) = struct_child(loadout, "WeaponGroupInfo") else {
        return Vec::new();
    };
    array_child(group_info, "WeaponGroups")
        .into_iter()
        .flat_map(|values| values.iter())
        .filter_map(|entry| {
            let entry = property_struct(entry)?;
            let slot = name_child(entry, "HardpointSlotID")?;
            Some(StockWeaponGroup {
                slot,
                g: [
                    bool_child(entry, "bWeaponGroup1").unwrap_or(false),
                    bool_child(entry, "bWeaponGroup2").unwrap_or(false),
                    bool_child(entry, "bWeaponGroup3").unwrap_or(false),
                    bool_child(entry, "bWeaponGroup4").unwrap_or(false),
                    bool_child(entry, "bWeaponGroup5").unwrap_or(false),
                    bool_child(entry, "bWeaponGroup6").unwrap_or(false),
                ],
            })
        })
        .collect()
}

fn equipment(loadout: &[Property]) -> Vec<StockEquipmentPart> {
    let Some(equipment) = struct_child(loadout, "Equipment") else {
        return Vec::new();
    };
    array_child(equipment, "MechPartEquipment")
        .into_iter()
        .flat_map(|values| values.iter())
        .filter_map(|entry| {
            let entry = property_struct(entry)?;
            let part = enum_child(entry, "MechPart")?;
            let items = array_child(entry, "SlottedEquipment")
                .into_iter()
                .flat_map(|values| values.iter())
                .filter_map(|item| {
                    let item = property_struct(item)?;
                    let slot_id = int_child(item, "SlotId")?;
                    let slot_type = primary_asset_name(item, &["SlotTypeAssetId", "Id"])?;
                    let (asset_type, name) =
                        primary_asset_pair(item, &["EquipmentData", "EquipmentId", "Id"])?;
                    Some(StockEquipmentItem {
                        asset_type,
                        name,
                        slot_id,
                        slot_type,
                    })
                })
                .collect();
            Some(StockEquipmentPart { part, items })
        })
        .collect()
}

fn health_map(properties: &[Property]) -> BTreeMap<String, f64> {
    properties
        .iter()
        .filter_map(|property| Some((property.get_name().get_content(), number_value(property)?)))
        .collect()
}

fn primary_asset_pair(properties: &[Property], path: &[&str]) -> Option<(String, String)> {
    let values = descend_struct(properties, path)?;
    Some((
        primary_asset_type_name(values)?,
        name_child(values, "PrimaryAssetName")?,
    ))
}

fn primary_asset_type_name(properties: &[Property]) -> Option<String> {
    let asset_type = struct_child(properties, "PrimaryAssetType")?;
    name_child(asset_type, "Name")
}

fn primary_asset_name(properties: &[Property], path: &[&str]) -> Option<String> {
    let values = descend_struct(properties, path)?;
    name_child(values, "PrimaryAssetName")
}

fn primary_asset_id_wrapper(
    properties: &[Property],
    field: &str,
    expected_type: &str,
) -> Option<PrimaryAssetIdWrapper> {
    let asset_name = primary_asset_name(properties, &[field, "Id"])?;
    Some(primary_asset_id_wrapper_empty(expected_type, &asset_name))
}

fn primary_asset_id_wrapper_empty(
    primary_asset_type: &str,
    primary_asset_name: &str,
) -> PrimaryAssetIdWrapper {
    PrimaryAssetIdWrapper {
        id: PrimaryAssetId {
            primary_asset_type: PrimaryAssetName {
                name: primary_asset_type.to_string(),
            },
            primary_asset_name: primary_asset_name.to_string(),
        },
    }
}

fn descend_struct<'a>(properties: &'a [Property], path: &[&str]) -> Option<&'a [Property]> {
    let mut current = properties;
    for field in path {
        current = struct_child(current, field)?;
    }
    Some(current)
}

fn struct_child<'a>(properties: &'a [Property], name: &str) -> Option<&'a [Property]> {
    property_struct(child(properties, name)?)
}

fn array_child<'a>(properties: &'a [Property], name: &str) -> Option<&'a [Property]> {
    match child(properties, name)? {
        Property::ArrayProperty(property) => Some(&property.value),
        _ => None,
    }
}

fn child<'a>(properties: &'a [Property], name: &str) -> Option<&'a Property> {
    properties
        .iter()
        .find(|property| property.get_name().get_content() == name)
}

fn property_struct(property: &Property) -> Option<&[Property]> {
    match property {
        Property::StructProperty(property) => Some(&property.value),
        _ => None,
    }
}

fn name_child(properties: &[Property], name: &str) -> Option<String> {
    match child(properties, name)? {
        Property::NameProperty(property) => Some(render_fname(&property.value)),
        _ => None,
    }
}

fn enum_child(properties: &[Property], name: &str) -> Option<String> {
    match child(properties, name)? {
        Property::EnumProperty(property) => property.value.as_ref().map(render_fname),
        _ => None,
    }
}

fn bool_child(properties: &[Property], name: &str) -> Option<bool> {
    match child(properties, name)? {
        Property::BoolProperty(property) => Some(property.value),
        _ => None,
    }
}

fn int_child(properties: &[Property], name: &str) -> Option<i32> {
    match child(properties, name)? {
        Property::IntProperty(property) => Some(property.value),
        _ => None,
    }
}

fn number_value(property: &Property) -> Option<f64> {
    match property {
        Property::FloatProperty(property) => Some(property.value.into_inner() as f64),
        Property::DoubleProperty(property) => Some(property.value.into_inner()),
        Property::IntProperty(property) => Some(property.value as f64),
        _ => None,
    }
}
