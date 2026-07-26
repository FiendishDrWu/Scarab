use std::collections::{BTreeMap, btree_map::Entry};

use super::{
    CompatibilityPlan, ExpectedMechPresentation, MechPresentation, MechPresentationContribution,
    ModSelector, ProcessorRegistration, SourceView,
};

const SUPPORTED_VERSION: &str = "3.6.8";
const SUPPORTED_BUILD_NUMBER: &str = "1000";
const STEAM_PUBLISHED_FILE_ID: &str = "3048850100";

const SELECTOR: ModSelector =
    ModSelector::new(&["YetAnotherLegendaryMech", STEAM_PUBLISHED_FILE_ID], None);

pub(super) const REGISTRATION: ProcessorRegistration = ProcessorRegistration::new(
    "yet-another-legendary-mech-3.6.8-build-1000",
    SELECTOR,
    SUPPORTED_VERSION,
    SUPPORTED_BUILD_NUMBER,
    process,
);

#[derive(Clone, Copy)]
struct ChassisRule {
    variants: &'static [&'static str],
    expected_chassis: &'static str,
    tons: u16,
    replacement_chassis: &'static str,
}

#[derive(Clone, Copy)]
struct HeroGroup {
    expected_chassis: &'static str,
    tons: u16,
    names: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Copy)]
struct PendingPresentation {
    expected_chassis: &'static str,
    tons: u16,
    is_hero: bool,
    replacement_chassis: &'static str,
    hero_name: Option<&'static str>,
}

const CHASSIS_RULES: &[ChassisRule] = &[
    ChassisRule {
        variants: &["MEGAS-XLR"],
        expected_chassis: "Shadow Hawk",
        tons: 100,
        replacement_chassis: "Megas",
    },
    ChassisRule {
        variants: &["MAD-X"],
        expected_chassis: "Marauder",
        tons: 100,
        replacement_chassis: "Marauder II",
    },
    ChassisRule {
        variants: &["YALM_CRG", "YALM_CRG-CA"],
        expected_chassis: "UrbanMech",
        tons: 10,
        replacement_chassis: "Corgi",
    },
    ChassisRule {
        variants: &[
            "VGL-II-1",
            "VGL-II-2",
            "VGL-II-3",
            "VGL-II-4",
            "VGL-II-RISC",
        ],
        expected_chassis: "Vaporeagle",
        tons: 45,
        replacement_chassis: "Vapor Eagle II",
    },
    ChassisRule {
        variants: &[
            "FMT-AL", "FMT-E", "FMT-F", "FMT-G", "FMT-H", "FMT-I", "FMT-J", "FMT-K", "FMT-M",
            "FMT-P", "FMT-R", "FMT-T",
        ],
        expected_chassis: "Firemoth",
        tons: 20,
        replacement_chassis: "Fire Moth",
    },
    ChassisRule {
        variants: &[
            "MIX-E", "MIX-EBD", "MIX-F", "MIX-G", "MIX-H", "MIX-I", "MIX-J", "MIX-K", "MIX-L",
            "MIX-M", "MIX-N", "MIX-T", "MIX-Z", "MIX-Z7",
        ],
        expected_chassis: "Mistlynx",
        tons: 25,
        replacement_chassis: "Mist Lynx",
    },
    ChassisRule {
        variants: &["YALM_SHC-NO"],
        expected_chassis: "Shadowcat",
        tons: 45,
        replacement_chassis: "Shadow Cat",
    },
    ChassisRule {
        variants: &["LM_TBR-HO"],
        expected_chassis: "Timberwolf",
        tons: 75,
        replacement_chassis: "Timber Wolf",
    },
    ChassisRule {
        variants: &[
            "VGL-1", "VGL-2", "VGL-3", "VGL-4", "VGL-5", "VGL-6", "VGL-7", "VGL-A", "VGL-GW",
            "VGL-H7", "VGL-MF", "VGL-RV",
        ],
        expected_chassis: "Vaporeagle",
        tons: 55,
        replacement_chassis: "Vapor Eagle",
    },
    ChassisRule {
        variants: &["DSHII-2", "DSHII-3", "DSHII-4", "DSHII-FF", "DSHII-PRIME"],
        expected_chassis: "DasherII",
        tons: 40,
        replacement_chassis: "Fire Moth II",
    },
    ChassisRule {
        variants: &["YALM_HBK-7X"],
        expected_chassis: "HunchbackIIC",
        tons: 50,
        replacement_chassis: "Hunchback IIC",
    },
    ChassisRule {
        variants: &[
            "YALM_HBK-IIC",
            "YALM_HBK-IIC-2",
            "YALM_HBK-IIC-3",
            "YALM_HBK-IIC-4",
            "YALM_HBK-IIC-4P",
            "YALM_HBK-IIC-5",
            "YALM_HBK-IIC-A",
            "YALM_HBK-IIC-B",
            "YALM_HBK-IIC-C",
            "YALM_HBK-IIC-DS",
            "YALM_HBK-IIC-DW",
            "YALM_HBK-IIC-Z7",
        ],
        expected_chassis: "YALMHunchbackIIC",
        tons: 50,
        replacement_chassis: "Hunchback IIC",
    },
    ChassisRule {
        variants: &[
            "LM_INC-2", "LM_INC-4", "LM_INC-5", "LM_INC-6", "LM_INC-8", "LM_INC-9",
        ],
        expected_chassis: "IncubusLM",
        tons: 30,
        replacement_chassis: "Incubus",
    },
    ChassisRule {
        variants: &["WHK-T"],
        expected_chassis: "WarhawkLM",
        tons: 85,
        replacement_chassis: "Warhawk",
    },
    ChassisRule {
        variants: &["LKC-1A"],
        expected_chassis: "LoaderKingCrab",
        tons: 65,
        replacement_chassis: "Loader King Crab",
    },
    ChassisRule {
        variants: &["MCII-H7", "MCII-MWK"],
        expected_chassis: "MadCatMk2LM",
        tons: 90,
        replacement_chassis: "TimberWolfMk2LM",
    },
];

const HERO_GROUPS: &[HeroGroup] = &[
    HeroGroup {
        expected_chassis: "Annihilator",
        tons: 100,
        names: &[("ANH-GZ", "Gausszilla"), ("ANH-SC", "Stone Crusher")],
    },
    HeroGroup {
        expected_chassis: "Atlas",
        tons: 100,
        names: &[("AS7-BIG", "Big Al"), ("AS7-W", "Warlord")],
    },
    HeroGroup {
        expected_chassis: "Bane",
        tons: 100,
        names: &[("BANE-L", "Leviathan")],
    },
    HeroGroup {
        expected_chassis: "Black Knight",
        tons: 75,
        names: &[("BL-X-KNT", "Red Reaper"), ("BL-X2-KNT", "Red Reaper II")],
    },
    HeroGroup {
        expected_chassis: "Bullshark",
        tons: 95,
        names: &[
            ("BSK-BD", "Black Death"),
            ("BSK-M", "Mako"),
            ("BSK-VK", "Void Killer"),
            ("BSK-WG", "War Ghoul"),
        ],
    },
    HeroGroup {
        expected_chassis: "Champion",
        tons: 60,
        names: &[("CHP-AP", "Apache")],
    },
    HeroGroup {
        expected_chassis: "Cyclops",
        tons: 90,
        names: &[("CP-AR", "Arges")],
    },
    HeroGroup {
        expected_chassis: "Catapult",
        tons: 65,
        names: &[("CPLT-FB", "Ferroblast")],
    },
    HeroGroup {
        expected_chassis: "Cataphract",
        tons: 70,
        names: &[("CTF-5MOC", "Naomi")],
    },
    HeroGroup {
        expected_chassis: "DasherII",
        tons: 40,
        names: &[("DSHII-FF", "Flurry Fire")],
    },
    HeroGroup {
        expected_chassis: "Executioner",
        tons: 95,
        names: &[("EXE-B-C-S", "Sovereign"), ("EXE-CRB", "Cherbi")],
    },
    HeroGroup {
        expected_chassis: "Firemoth",
        tons: 20,
        names: &[("FMT-AL", "Aletha Kabrinski")],
    },
    HeroGroup {
        expected_chassis: "Hunchback",
        tons: 50,
        names: &[("HBK-LEG", "Legionnaire")],
    },
    HeroGroup {
        expected_chassis: "Hellbringer",
        tons: 65,
        names: &[("HBR-VG", "Virago")],
    },
    HeroGroup {
        expected_chassis: "Hellfire",
        tons: 60,
        names: &[("HLF-VD", "Void")],
    },
    HeroGroup {
        expected_chassis: "Huntsman",
        tons: 50,
        names: &[("HMN-PKT", "Pakhet")],
    },
    HeroGroup {
        expected_chassis: "King Crab",
        tons: 100,
        names: &[("KGC-A", "Argent")],
    },
    HeroGroup {
        expected_chassis: "Longbow",
        tons: 85,
        names: &[("LGB-LNG", "Spitfire")],
    },
    HeroGroup {
        expected_chassis: "Incubus",
        tons: 30,
        names: &[("LM_INC-Z0", "Zero")],
    },
    HeroGroup {
        expected_chassis: "LMRiflemanIIC",
        tons: 65,
        names: &[("LM_RFL-IIC-CH", "Chironex")],
    },
    HeroGroup {
        expected_chassis: "Timberwolf",
        tons: 75,
        names: &[("LM_TBR-HO", "Howl")],
    },
    HeroGroup {
        expected_chassis: "Marauder",
        tons: 75,
        names: &[("MAD-BL", "Blight")],
    },
    HeroGroup {
        expected_chassis: "MarauderIICLM",
        tons: 85,
        names: &[("MAD-IIC-DN", "Dreadnought")],
    },
    HeroGroup {
        expected_chassis: "MadCatMk2LM",
        tons: 90,
        names: &[("MCII-MWK", "Moonwalker")],
    },
    HeroGroup {
        expected_chassis: "Mistlynx",
        tons: 25,
        names: &[("MIX-EBD", "Ebon Dragoon")],
    },
    HeroGroup {
        expected_chassis: "Phoenix Hawk",
        tons: 45,
        names: &[("PXH-S", "Spectre")],
    },
    HeroGroup {
        expected_chassis: "Quickdraw",
        tons: 60,
        names: &[("QKD-D", "Desperada")],
    },
    HeroGroup {
        expected_chassis: "Sunder",
        tons: 90,
        names: &[("SD1-OA-CL", "Coleman")],
    },
    HeroGroup {
        expected_chassis: "Shadow Hawk",
        tons: 55,
        names: &[("SHD-S", "Scattershot")],
    },
    HeroGroup {
        expected_chassis: "LMSupernova",
        tons: 90,
        names: &[
            ("SNV-BLR", "Boiler"),
            ("SNV-Q", "Quasar"),
            ("SNV-SR", "Seraph"),
        ],
    },
    HeroGroup {
        expected_chassis: "LMStoneRhino",
        tons: 100,
        names: &[("SR-AK", "Aksum")],
    },
    HeroGroup {
        expected_chassis: "Stalker",
        tons: 85,
        names: &[("STK-WU", "War Emu")],
    },
    HeroGroup {
        expected_chassis: "Vaporeagle",
        tons: 55,
        names: &[
            ("VGL-GW", "Gore Wing"),
            ("VGL-MF", "Mean Frog"),
            ("VGL-RV", "Rival"),
        ],
    },
    HeroGroup {
        expected_chassis: "Viper",
        tons: 40,
        names: &[
            ("VPR-MD", "Medusa"),
            ("VPR-MF", "Mean Frog II"),
            ("VPR-S", "Scaleshot"),
        ],
    },
    HeroGroup {
        expected_chassis: "Victor",
        tons: 80,
        names: &[("VTR-LDT", "Li-Dok-To")],
    },
    HeroGroup {
        expected_chassis: "Warhawk",
        tons: 85,
        names: &[("WHK-K", "Kasai"), ("WHK-NQ", "Nanqu"), ("WHK-TR", "Tara")],
    },
    HeroGroup {
        expected_chassis: "Wolverine",
        tons: 55,
        names: &[("WVR-S", "Starshot")],
    },
    HeroGroup {
        expected_chassis: "UrbanMech",
        tons: 10,
        names: &[("YALM_CRG-CA", "Cardigan")],
    },
    HeroGroup {
        expected_chassis: "YALMHunchbackIIC",
        tons: 50,
        names: &[
            ("YALM_HBK-IIC-DS", "Dwarf Star"),
            ("YALM_HBK-IIC-DW", "Death Wish"),
        ],
    },
    HeroGroup {
        expected_chassis: "Mad Dog",
        tons: 60,
        names: &[("YALM_MDD-SI", "Sigma")],
    },
    HeroGroup {
        expected_chassis: "Shadowcat",
        tons: 45,
        names: &[("YALM_SHC-NO", "Noble")],
    },
    HeroGroup {
        expected_chassis: "Urbanmech-IIC",
        tons: 30,
        names: &[
            ("YALM_UM-IIC-IC", "Ironclad"),
            ("YALM_UM-IIC-WD", "Wild Dog"),
        ],
    },
];

const UNNAMED_HERO_VARIANTS: &[&str] = &["MEGAS-XLR"];

fn process(source: &SourceView<'_>) -> Result<CompatibilityPlan, String> {
    validate_steam_identity(source)?;
    let presentations = build_presentations()?;
    Ok(CompatibilityPlan {
        mech_presentations: presentations
            .into_iter()
            .map(|(variant, presentation)| MechPresentationContribution {
                variant: variant.to_string(),
                expected: ExpectedMechPresentation {
                    chassis: presentation.expected_chassis.to_string(),
                    tons: Some(presentation.tons),
                    is_hero: presentation.is_hero,
                },
                replacement: MechPresentation {
                    chassis: presentation.replacement_chassis.to_string(),
                    tons: Some(presentation.tons),
                    hero_name: presentation.hero_name.map(str::to_string),
                },
            })
            .collect(),
    })
}

fn validate_steam_identity(source: &SourceView<'_>) -> Result<(), String> {
    if let Some(steam_id) = source.identity.steam_published_file_id.as_deref()
        && steam_id != STEAM_PUBLISHED_FILE_ID
    {
        return Err(format!(
            "unexpected Yet Another Legendary Mech Steam published-file ID `{steam_id}`"
        ));
    }
    Ok(())
}

fn build_presentations() -> Result<BTreeMap<&'static str, PendingPresentation>, String> {
    let mut presentations = BTreeMap::new();

    for rule in CHASSIS_RULES {
        for &variant in rule.variants {
            let presentation = PendingPresentation {
                expected_chassis: rule.expected_chassis,
                tons: rule.tons,
                is_hero: hero_name_for(variant).is_some()
                    || UNNAMED_HERO_VARIANTS.contains(&variant),
                replacement_chassis: rule.replacement_chassis,
                hero_name: None,
            };
            if presentations.insert(variant, presentation).is_some() {
                return Err(format!(
                    "internal YALM compatibility table contains duplicate chassis rule for `{variant}`"
                ));
            }
        }
    }

    for group in HERO_GROUPS {
        for &(variant, hero_name) in group.names {
            match presentations.entry(variant) {
                Entry::Vacant(entry) => {
                    entry.insert(PendingPresentation {
                        expected_chassis: group.expected_chassis,
                        tons: group.tons,
                        is_hero: true,
                        replacement_chassis: group.expected_chassis,
                        hero_name: Some(hero_name),
                    });
                }
                Entry::Occupied(mut entry) => {
                    let presentation = entry.get_mut();
                    if presentation.expected_chassis != group.expected_chassis
                        || presentation.tons != group.tons
                        || !presentation.is_hero
                    {
                        return Err(format!(
                            "internal YALM compatibility evidence disagrees for Hero variant `{variant}`"
                        ));
                    }
                    if presentation.hero_name.replace(hero_name).is_some() {
                        return Err(format!(
                            "internal YALM compatibility table contains duplicate Hero rule for `{variant}`"
                        ));
                    }
                }
            }
        }
    }

    for &variant in UNNAMED_HERO_VARIANTS {
        let Some(presentation) = presentations.get(variant) else {
            return Err(format!(
                "internal YALM compatibility table is missing unnamed Hero `{variant}`"
            ));
        };
        if !presentation.is_hero || presentation.hero_name.is_some() {
            return Err(format!(
                "internal YALM compatibility table mishandles unnamed Hero `{variant}`"
            ));
        }
    }

    Ok(presentations)
}

fn hero_name_for(variant: &str) -> Option<&'static str> {
    HERO_GROUPS
        .iter()
        .flat_map(|group| group.names.iter())
        .find_map(|(candidate, name)| (*candidate == variant).then_some(*name))
}
