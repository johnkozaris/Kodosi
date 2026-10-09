func generateSessionName() -> String {
    let adjectives = [
        "Amber", "Arctic", "Azure", "Brass", "Bronze", "Cedar", "Cobalt",
        "Copper", "Coral", "Crimson", "Crystal", "Dusk", "Ember", "Fern",
        "Flint", "Frost", "Golden", "Granite", "Hazel", "Indigo", "Iron",
        "Ivory", "Jade", "Jasper", "Juniper", "Lapis", "Maple", "Marble",
        "Misty", "Moss", "Nimble", "Obsidian", "Onyx", "Opal", "Pearl",
        "Pine", "Quartz", "Raven", "Rosewood", "Ruby", "Rustic", "Sage",
        "Sandy", "Scarlet", "Shadow", "Silver", "Slate", "Steel", "Stone",
        "Swift", "Tawny", "Timber", "Topaz", "Verdant", "Violet", "Willow",
    ]

    let animals = [
        "Badger", "Bear", "Bobcat", "Cardinal", "Condor", "Cougar", "Crane",
        "Crow", "Deer", "Eagle", "Elk", "Falcon", "Finch", "Fox", "Gecko",
        "Goose", "Hawk", "Heron", "Ibis", "Jaguar", "Jay", "Kestrel",
        "Lark", "Leopard", "Lynx", "Marten", "Merlin", "Moose", "Newt",
        "Orca", "Osprey", "Otter", "Owl", "Panther", "Pelican", "Puma",
        "Quail", "Raccoon", "Raven", "Robin", "Salmon", "Seal", "Sparrow",
        "Stork", "Swan", "Tiger", "Viper", "Wolf", "Wren",
    ]

    let adj = adjectives.randomElement() ?? "New"
    let animal = animals.randomElement() ?? "Terminal"
    return "\(adj) \(animal)"
}
