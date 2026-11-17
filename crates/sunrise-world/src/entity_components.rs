//! `Entity.get(DataComponentType)`: the components an entity exposes as a
//! `DataComponentGetter`, read from its saved NBT, which both the server's and
//! the client's entities can produce. Values are in each component's codec
//! form (registry ids, enum names, dye names), as an item stack stores them.
//! The base entity's `custom_name` and `custom_data` are the caller's.
//! Vanilla 26.3 attaches no components to entity types, so there are no type
//! defaults to add.
use sunrise_core::nbt::{Compound, Tag};

/// `DyeColor` names by id.
pub const DYE_NAMES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];
/// `Axolotl.Variant` names, by id.
pub const AXOLOTL_VARIANTS: [&str; 5] = ["lucy", "wild", "gold", "cyan", "blue"];
/// `Salmon.Variant` names, by id.
pub const SALMON_SIZES: [&str; 3] = ["small", "medium", "large"];
/// `TropicalFish.Pattern` names in enum order: the six small patterns, then
/// the six large ones.
pub const TROPICAL_PATTERNS: [&str; 12] = [
    "kob",
    "sunstreak",
    "snooper",
    "dasher",
    "brinely",
    "spotty",
    "flopper",
    "stripey",
    "glitter",
    "blockfish",
    "betty",
    "clayfish",
];
const HORSE_VARIANTS: [&str; 7] = [
    "white",
    "creamy",
    "chestnut",
    "brown",
    "black",
    "gray",
    "dark_brown",
];
const LLAMA_VARIANTS: [&str; 4] = ["creamy", "white", "brown", "gray"];
const PARROT_VARIANTS: [&str; 5] = ["red_blue", "blue", "green", "yellow_blue", "gray"];

fn int(nbt: &Tag, key: &str) -> Option<i64> {
    match nbt.get(key)? {
        Tag::Byte(v) => Some(i64::from(*v)),
        Tag::Short(v) => Some(i64::from(*v)),
        Tag::Int(v) => Some(i64::from(*v)),
        Tag::Long(v) => Some(*v),
        _ => None,
    }
}

fn name(names: &[&str], id: usize) -> Tag {
    Tag::String(names[id].into())
}

/// `DyeColor.byId`: an id outside 0..=15 is white.
fn dye(id: i64) -> Tag {
    name(
        &DYE_NAMES,
        usize::try_from(id).ok().filter(|id| *id < 16).unwrap_or(0),
    )
}

/// The components `Entity.get` returns for an entity of type `kind` with saved
/// data `nbt`, past the base entity's. A component whose field the save lacks
/// is the field's default, or absent where vanilla has none.
pub fn type_components(kind: &str, nbt: &Tag) -> Compound {
    let mut out = Compound::new();
    let mut put = |component: &str, value: Option<Tag>| {
        if let Some(value) = value {
            out.insert(component.into(), value);
        }
    };
    // The saved field is already the component's value (a registry id, an
    // enum name or a codec's own form).
    let copy = |key: &str| nbt.get(key).cloned();
    // A saved id into `names`; an id out of range follows the codec's rule.
    let by_id = |key: &str, names: &[&str], clamped: bool| {
        let id = int(nbt, key).unwrap_or(0);
        let index = if clamped {
            id.clamp(0, names.len() as i64 - 1) as usize
        } else {
            usize::try_from(id)
                .ok()
                .filter(|id| *id < names.len())
                .unwrap_or(0)
        };
        Some(name(names, index))
    };
    match kind.strip_prefix("minecraft:").unwrap_or(kind) {
        species @ ("cat" | "wolf") => {
            put(&format!("minecraft:{species}/variant"), copy("variant"));
            put(
                &format!("minecraft:{species}/sound_variant"),
                copy("sound_variant"),
            );
            let collar = Some(dye(int(nbt, "CollarColor").unwrap_or(14)));
            put(&format!("minecraft:{species}/collar"), collar);
        }
        species @ ("pig" | "cow" | "chicken") => {
            put(&format!("minecraft:{species}/variant"), copy("variant"));
            put(
                &format!("minecraft:{species}/sound_variant"),
                copy("sound_variant"),
            );
        }
        "frog" => put("minecraft:frog/variant", copy("variant")),
        "zombie_nautilus" => put("minecraft:zombie_nautilus/variant", copy("variant")),
        "painting" => put("minecraft:painting/variant", copy("variant")),
        "fox" => put("minecraft:fox/variant", copy("Type")),
        "mooshroom" => put("minecraft:mooshroom/variant", copy("Type")),
        "salmon" => put("minecraft:salmon/size", copy("type")),
        "sheep" => put(
            "minecraft:sheep/color",
            Some(dye(int(nbt, "Color").unwrap_or(0))),
        ),
        // `Shulker.getColor`: 16 is no colour, and so is any other id past 15.
        "shulker" => {
            if let Some(color @ 0..=15) = int(nbt, "Color") {
                put("minecraft:shulker/color", Some(dye(color)));
            }
        }
        "rabbit" => {
            // `Rabbit.Variant.LEGACY_CODEC`: a sparse id map, brown otherwise.
            const RABBIT: [(i64, &str); 6] = [
                (0, "brown"),
                (1, "white"),
                (2, "black"),
                (3, "white_splotched"),
                (4, "gold"),
                (5, "salt"),
            ];
            let id = int(nbt, "RabbitType").unwrap_or(0);
            let variant = if id == 99 {
                "evil"
            } else {
                RABBIT
                    .iter()
                    .find(|(i, _)| *i == id)
                    .map_or("brown", |v| v.1)
            };
            put(
                "minecraft:rabbit/variant",
                Some(Tag::String(variant.into())),
            );
        }
        // `Horse.getVariant`: the coat is the low byte of the packed variant,
        // wrapped over the seven coats.
        "horse" => {
            let coat = int(nbt, "Variant").unwrap_or(0) & 0xFF;
            let coat = name(&HORSE_VARIANTS, coat as usize % HORSE_VARIANTS.len());
            put("minecraft:horse/variant", Some(coat));
        }
        "llama" | "trader_llama" => put(
            "minecraft:llama/variant",
            by_id("Variant", &LLAMA_VARIANTS, true),
        ),
        "parrot" => put(
            "minecraft:parrot/variant",
            by_id("Variant", &PARROT_VARIANTS, true),
        ),
        "axolotl" => put(
            "minecraft:axolotl/variant",
            by_id("Variant", &AXOLOTL_VARIANTS, false),
        ),
        // `TropicalFish.Variant`: pattern size, pattern index, base and
        // pattern colour packed in one int; an unknown pattern is a kob.
        "tropical_fish" => {
            let packed = int(nbt, "Variant").unwrap_or(0);
            let (size, index) = (packed & 0xFF, (packed >> 8) & 0xFF);
            let pattern = if size <= 1 && index < 6 {
                (size * 6 + index) as usize
            } else {
                0
            };
            put(
                "minecraft:tropical_fish/pattern",
                Some(name(&TROPICAL_PATTERNS, pattern)),
            );
            put(
                "minecraft:tropical_fish/base_color",
                Some(dye((packed >> 16) & 0xFF)),
            );
            put(
                "minecraft:tropical_fish/pattern_color",
                Some(dye((packed >> 24) & 0xFF)),
            );
        }
        "villager" | "zombie_villager" => {
            let variant = nbt.get("VillagerData").and_then(|data| data.get("type"));
            put("minecraft:villager/variant", variant.cloned());
        }
        "cushion" => put("minecraft:cushion/color", copy("color")),
        "mannequin" => put("minecraft:profile", copy("profile")),
        "area_effect_cloud" => {
            put("minecraft:potion_contents", copy("potion_contents"));
            put(
                "minecraft:potion_duration_scale",
                copy("potion_duration_scale"),
            );
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compound<const N: usize>(entries: [(&str, Tag); N]) -> Tag {
        Tag::compound(entries)
    }
    fn text(components: &Compound, id: &str) -> Option<Tag> {
        components.get(&sunrise_core::nbt::JavaString::from(id)).cloned()
    }

    #[test]
    fn components_follow_each_types_get() {
        let cat = type_components(
            "minecraft:cat",
            &compound([
                ("variant", Tag::String("minecraft:tabby".into())),
                ("CollarColor", Tag::Byte(3)),
            ]),
        );
        assert_eq!(
            text(&cat, "minecraft:cat/variant"),
            Some(Tag::String("minecraft:tabby".into()))
        );
        assert_eq!(
            text(&cat, "minecraft:cat/collar"),
            Some(Tag::String("light_blue".into()))
        );

        let sheep = type_components("minecraft:sheep", &compound([("Color", Tag::Byte(11))]));
        assert_eq!(
            text(&sheep, "minecraft:sheep/color"),
            Some(Tag::String("blue".into()))
        );

        // 0x0B_03_01_00: pattern size 0 index 1, base 3, pattern 11.
        let fish = type_components(
            "minecraft:tropical_fish",
            &compound([("Variant", Tag::Int(0x0B03_0100))]),
        );
        assert_eq!(
            text(&fish, "minecraft:tropical_fish/pattern"),
            Some(Tag::String("sunstreak".into()))
        );
        assert_eq!(
            text(&fish, "minecraft:tropical_fish/base_color"),
            Some(Tag::String("light_blue".into()))
        );
        assert_eq!(
            text(&fish, "minecraft:tropical_fish/pattern_color"),
            Some(Tag::String("blue".into()))
        );

        // The horse's coat is the packed variant's low byte; markings are not part of it.
        let horse = type_components(
            "minecraft:horse",
            &compound([("Variant", Tag::Int(0x0402))]),
        );
        assert_eq!(
            text(&horse, "minecraft:horse/variant"),
            Some(Tag::String("chestnut".into()))
        );

        let shulker = type_components("minecraft:shulker", &compound([("Color", Tag::Byte(16))]));
        assert!(shulker.is_empty());
        assert!(type_components("minecraft:zombie", &compound([])).is_empty());
    }
}
