use std::collections::HashSet;

const ADJECTIVES: [&str; 55] = [
    "Amber", "Arctic", "Azure", "Brass", "Bronze", "Cedar", "Cobalt", "Copper", "Coral", "Crimson",
    "Crystal", "Dusk", "Ember", "Fern", "Flint", "Frost", "Golden", "Granite", "Hazel", "Indigo",
    "Iron", "Ivory", "Jade", "Jasper", "Juniper", "Lapis", "Maple", "Marble", "Misty", "Moss",
    "Nimble", "Obsidian", "Onyx", "Opal", "Pearl", "Pine", "Quartz", "Rosewood", "Ruby", "Rustic",
    "Sage", "Sandy", "Scarlet", "Shadow", "Silver", "Slate", "Steel", "Stone", "Swift", "Tawny",
    "Timber", "Topaz", "Verdant", "Violet", "Willow",
];

const ANIMALS: [&str; 49] = [
    "Badger", "Bear", "Bobcat", "Cardinal", "Condor", "Cougar", "Crane", "Crow", "Deer", "Eagle",
    "Elk", "Falcon", "Finch", "Fox", "Gecko", "Goose", "Hawk", "Heron", "Ibis", "Jaguar", "Jay",
    "Kestrel", "Lark", "Leopard", "Lynx", "Marten", "Merlin", "Moose", "Newt", "Orca", "Osprey",
    "Otter", "Owl", "Panther", "Pelican", "Puma", "Quail", "Raccoon", "Raven", "Robin", "Salmon",
    "Seal", "Sparrow", "Stork", "Swan", "Tiger", "Viper", "Wolf", "Wren",
];

pub(crate) fn new_terminal(taken: &HashSet<&str>) -> String {
    pair(taken, |adjective, animal| format!("{adjective} {animal}"))
}

/// The terminal on a new branch takes the branch's name, so the branch avoids the names of
/// terminals too.
pub(crate) fn new_branch(taken: &HashSet<&str>) -> String {
    pair(taken, |adjective, animal| {
        format!("{}-{}", adjective.to_lowercase(), animal.to_lowercase())
    })
}

pub(crate) fn unused(name: String, taken: &HashSet<&str>) -> String {
    if !taken.contains(name.as_str()) {
        return name;
    }
    (2..=taken.len() + 1)
        .map(|number| format!("{name} {number}"))
        .find(|candidate| !taken.contains(candidate.as_str()))
        .unwrap_or(name)
}

fn pair(taken: &HashSet<&str>, form: impl Fn(&str, &str) -> String) -> String {
    let pairs = ADJECTIVES.len() * ANIMALS.len();
    let named = |index: usize| {
        form(
            ADJECTIVES[index / ANIMALS.len()],
            ANIMALS[index % ANIMALS.len()],
        )
    };
    let first = random_below(pairs);
    (first..first + pairs)
        .map(|index| named(index % pairs))
        .find(|name| !taken.contains(name.as_str()))
        .unwrap_or_else(|| unused(named(first), taken))
}

fn random_below(count: usize) -> usize {
    let mut random = [0; size_of::<usize>()];
    if aws_lc_rs::rand::fill(&mut random).is_err() {
        return 0;
    }
    usize::from_le_bytes(random) % count
}
