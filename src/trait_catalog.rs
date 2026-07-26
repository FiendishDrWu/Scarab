use serde::{Deserialize, Serialize};
use unreal_asset::properties::{Property, PropertyDataTrait};

use crate::unreal_name::render_fname;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraitCategory {
    Pilot,
    Mech,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JjTrait {
    pub category: TraitCategory,
    pub asset_name: String,
    pub friendly_label: String,
}

pub(crate) fn trait_from_unreal_export(
    class_name: &str,
    asset_name: &str,
    properties: &[Property],
) -> Option<JjTrait> {
    let category = trait_category_from_class(class_name)?;
    if is_non_catalog_trait_asset(asset_name) {
        return None;
    }
    let parsed_label =
        label_from_properties(properties).filter(|label| !label.eq_ignore_ascii_case(asset_name));
    let friendly_label = derived_label_from_asset_name(asset_name)
        .or(parsed_label)
        .filter(|label| !label.eq_ignore_ascii_case(asset_name))?;

    Some(JjTrait {
        category,
        asset_name: asset_name.to_string(),
        friendly_label,
    })
}

pub(crate) fn trait_category_from_class(class_name: &str) -> Option<TraitCategory> {
    let lower = class_name.to_ascii_lowercase();
    if lower.contains("mwpilottraitdataasset") {
        Some(TraitCategory::Pilot)
    } else if lower.contains("mwmechtraitdataasset") {
        Some(TraitCategory::Mech)
    } else {
        None
    }
}

fn label_from_properties(properties: &[Property]) -> Option<String> {
    const PREFERRED_NAMES: &[&str] = &[
        "PilotTraitName",
        "MechTraitName",
        "DisplayName",
        "TraitName",
        "TraitDisplayName",
        "FriendlyName",
        "Title",
    ];

    for name in PREFERRED_NAMES {
        if let Some(label) = label_property_by_exact_name(properties, name) {
            return Some(label);
        }
    }

    properties.iter().find_map(label_property_by_labelish_name)
}

fn label_property_by_exact_name(properties: &[Property], name: &str) -> Option<String> {
    for property in properties {
        if property.get_name().get_content().eq_ignore_ascii_case(name)
            && let Some(label) = string_from_property(property)
        {
            return Some(label);
        }
        if let Some(label) = child_label_by_exact_name(property, name) {
            return Some(label);
        }
    }
    None
}

fn child_label_by_exact_name(property: &Property, name: &str) -> Option<String> {
    match property {
        Property::StructProperty(property) => label_property_by_exact_name(&property.value, name),
        Property::ArrayProperty(property) => label_property_by_exact_name(&property.value, name),
        _ => None,
    }
}

fn label_property_by_labelish_name(property: &Property) -> Option<String> {
    let name = property.get_name().get_content();
    let lower = name.to_ascii_lowercase();
    if (lower.contains("display") || lower.contains("label") || lower.contains("traitname"))
        && !lower.contains("description")
        && !lower.contains("tooltip")
        && let Some(label) = string_from_property(property)
    {
        return Some(label);
    }

    match property {
        Property::StructProperty(property) => property
            .value
            .iter()
            .find_map(label_property_by_labelish_name),
        Property::ArrayProperty(property) => property
            .value
            .iter()
            .find_map(label_property_by_labelish_name),
        _ => None,
    }
}

fn string_from_property(property: &Property) -> Option<String> {
    match property {
        Property::TextProperty(property) => property
            .culture_invariant_string
            .as_deref()
            .or(property.value.as_deref())
            .and_then(non_empty_label),
        Property::StrProperty(property) => property.value.as_deref().and_then(non_empty_label),
        Property::NameProperty(property) => non_empty_label(&render_fname(&property.value)),
        _ => None,
    }
}

fn non_empty_label(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_non_catalog_trait_asset(asset_name: &str) -> bool {
    asset_name.eq_ignore_ascii_case("TestTrait")
}

fn derived_label_from_asset_name(asset_name: &str) -> Option<String> {
    let stem = asset_name
        .strip_suffix("_PilotTrait")
        .or_else(|| asset_name.strip_suffix("_Trait"))
        .unwrap_or(asset_name);

    known_trait_label_from_stem(stem).or_else(|| embedded_trait_label_from_stem(stem))
}

fn embedded_trait_label_from_stem(stem: &str) -> Option<String> {
    stem.match_indices('_')
        .filter_map(|(index, _)| stem.get(index + 1..))
        .find_map(known_trait_label_from_stem)
}

fn known_trait_label_from_stem(stem: &str) -> Option<String> {
    if let Some(rest) = stem.strip_prefix("Affinity_") {
        let subject = rest
            .strip_suffix("_Chassis")
            .or_else(|| rest.strip_prefix("Freeman_"))
            .unwrap_or(rest);
        return Some(format!(
            "{} 'Mech Affinity",
            affinity_subject_label(subject)
        ));
    }

    if let Some(rest) = stem.strip_prefix("Background_Clan") {
        return clan_label(rest).map(|clan| format!("{clan} Origins"));
    }

    if stem == "Background_Periphery" {
        return Some("Periphery Origins".to_string());
    }

    if let Some(tech_base) = stem.strip_prefix("Connoisseur_") {
        return Some(format!("{} Connoisseur", tech_base_label(tech_base)));
    }

    for (prefix, formatter) in [
        (
            "Criminal_",
            house_criminal_label as fn(&str, &str) -> String,
        ),
        ("Hatred_", house_hatred_label),
        ("Noble_", house_noble_label),
        ("Patriot_", house_patriot_label),
    ] {
        if let Some(house) = stem.strip_prefix(prefix) {
            return Some(formatter(house, house_label(house)));
        }
    }

    if let Some(weapon) = stem.strip_prefix("Expert_") {
        return Some(format!("{} Expert", weapon_trait_label(weapon)));
    }

    if let Some(weapon) = stem.strip_prefix("Master_") {
        return Some(format!("{} Master", weapon_trait_label(weapon)));
    }

    if let Some(weapon) = stem.strip_prefix("Specialist_") {
        return Some(match weapon {
            "Autocannon" => "standard Autocannon Specialist".to_string(),
            "PointBlank" => "Point Blank Specialist".to_string(),
            other => format!("{} Specialist", weapon_trait_label(other)),
        });
    }

    if let Some(sponsor) = stem.strip_prefix("Sponsor_") {
        return Some(format!("{} Sponsorship", sponsor_label(sponsor)));
    }

    if let Some(level) = stem.strip_prefix("TargetingComputerMk") {
        return Some(format!("Targeting Computer Mk{level}"));
    }

    None
}

fn affinity_subject_label(subject: &str) -> String {
    match subject {
        "BlackHawk" => "Black Hawk",
        "BlackKnight" => "Black Knight",
        "Bullshark" | "BullShark" => "Bull Shark",
        "CauldronBorn" => "Cauldron-Born",
        "Firemoth" => "Dasher",
        "Hatamoto-Chi" => "Hatamoto-Chi",
        "JagerMech" => "JagerMech",
        "KingCrab" => "King Crab",
        "MadCat" => "Mad Cat",
        "ManOWar" => "Man O' War",
        "MarauderII" => "Marauder II",
        "NightGyr" => "Night Gyr",
        "PhoenixHawk" => "Phoenix Hawk",
        "ShadowHawk" => "Shadow Hawk",
        "Shadowcat" => "Shadow Cat",
        other => other,
    }
    .to_string()
}

fn clan_label(clan: &str) -> Option<&'static str> {
    match clan {
        "DiamondShark" => Some("Clan Diamond Shark"),
        "GhostBear" => Some("Clan Ghost Bear"),
        "JadeFalcon" => Some("Clan Jade Falcon"),
        "NovaCat" => Some("Clan Nova Cat"),
        "SmokeJaguar" => Some("Clan Smoke Jaguar"),
        "SteelViper" => Some("Clan Steel Viper"),
        "Wolf" => Some("Clan Wolf"),
        _ => None,
    }
}

fn house_label(house: &str) -> &str {
    faction_label(house)
}

fn faction_label(faction: &str) -> &str {
    match faction {
        "Clans" => "Clans",
        "Marik" => "Free Worlds League",
        "Davion" => "House Davion",
        "Kurita" => "House Kurita",
        "Liao" => "House Liao",
        "Outlaws" => "Outlaws",
        "Periphery" => "Periphery",
        "Rasalhague" => "Rasalhague",
        "Steiner" => "House Steiner",
        "WordofBlake" => "Word of Blake",
        other => other,
    }
}

fn house_criminal_label(_house: &str, label: &str) -> String {
    format!("Criminal ({label})")
}

fn house_hatred_label(_house: &str, label: &str) -> String {
    format!("Hates {label}")
}

fn house_noble_label(_house: &str, label: &str) -> String {
    format!("{label} Noble")
}

fn house_patriot_label(_house: &str, label: &str) -> String {
    format!("{label} Patriot")
}

fn weapon_trait_label(weapon: &str) -> &str {
    match weapon {
        "AC10" | "Autocannon10" => "AC/10",
        "AC2" | "Autocannon2" => "AC/2",
        "AC20" | "Autocannon20" => "AC/20",
        "AC5" | "Autocannon5" => "AC/5",
        "ATM" => "ATM",
        "ATM3" => "ATM 3",
        "ATM6" => "ATM 6",
        "ATM9" => "ATM 9",
        "ATM12" => "ATM 12",
        "ArrowIV" => "Arrow IV",
        "Artillery" => "Artillery",
        "Gauss" => "Gauss",
        "GaussRifle" => "Gauss Rifle",
        "HRifle" => "Heavy Rifle",
        "HyperAutocannon" => "Hyper Autocannon",
        "LargeLaser" => "Large Laser",
        "LBXAC" => "LBX AC",
        "LightAutocannon" => "Light Autocannon",
        "LongTom" => "Long Tom",
        "LRM10" => "LRM 10",
        "LRM15" => "LRM 15",
        "LRM20" => "LRM 20",
        "LRM5" => "LRM 5",
        "LRifle" => "Light Rifle",
        "MediumLaser" => "Medium Laser",
        "MRifle" => "Medium Rifle",
        "MRM" => "MRM",
        "NARC" => "NARC",
        "Plasma" => "Plasma",
        "PlasmaCannon" => "Plasma Cannon",
        "PlasmaRifle" => "Plasma Rifle",
        "ProtoAutocannon" => "Proto Autocannon",
        "PPC" => "PPC",
        "Rifle" => "Rifle",
        "Rocket" => "Rocket",
        "RotaryAutocannon" => "Rotary Autocannon",
        "SmallLaser" => "Small Laser",
        "Sniper" => "Sniper",
        "SRM2" => "SRM 2",
        "SRM4" => "SRM 4",
        "SRM6" => "SRM 6",
        "Thumper" => "Thumper",
        "Thunderbolt" => "Thunderbolt",
        "UltraAC" => "Ultra AC",
        "LRM" => "LRM",
        "Laser" => "Laser",
        "SRM" => "SRM",
        other => other,
    }
}

fn tech_base_label(tech_base: &str) -> &str {
    match tech_base {
        "Clan" => "Clan Tech",
        "LosTech" => "Lostech",
        "Retro" => "Retro Tech",
        other => other,
    }
}

fn sponsor_label(sponsor: &str) -> &str {
    match sponsor {
        "Bullhead" => "Bullhead",
        "Earthwerks" => "Earthwerks",
        "HighImpact" => "High Impact",
        "JumpJet" => "Jump Jet",
        "Musclebound" => "Musclebound",
        "SolarisArms" => "Solaris Arms",
        "TripleF" => "Triple-F",
        "VEST" => "VEST",
        other => other,
    }
}
