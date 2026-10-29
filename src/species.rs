use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Species {
    pub id: usize,
    pub name: &'static str,
    pub files: &'static str,
    pub color: [u8; 3],
}

pub const SPECIES: [Species; 32] = [
    Species {
        id: 0,
        name: "Woodland fern",
        files: "Rust .rs",
        color: [124, 140, 81],
    },
    Species {
        id: 1,
        name: "Cornflower",
        files: "TypeScript .ts .tsx",
        color: [113, 148, 171],
    },
    Species {
        id: 2,
        name: "Sunflower",
        files: "JavaScript .js .jsx",
        color: [202, 169, 88],
    },
    Species {
        id: 3,
        name: "Lavender",
        files: "Python .py",
        color: [151, 138, 166],
    },
    Species {
        id: 4,
        name: "Jade succulent",
        files: "CSS .css",
        color: [133, 161, 121],
    },
    Species {
        id: 5,
        name: "Scots pine",
        files: "C# F# OCaml",
        color: [105, 134, 100],
    },
    Species {
        id: 6,
        name: "Daisies",
        files: "Tests and specs",
        color: [227, 223, 194],
    },
    Species {
        id: 7,
        name: "Birch sapling",
        files: "Documentation .md .txt",
        color: [170, 182, 133],
    },
    Species {
        id: 8,
        name: "River stones",
        files: "Dotfiles and other config",
        color: [151, 157, 143],
    },
    Species {
        id: 9,
        name: "Mushrooms",
        files: "Lockfiles and dependencies",
        color: [164, 125, 96],
    },
    Species {
        id: 10,
        name: "Oat grass",
        files: "Dockerfile Makefile Justfile",
        color: [185, 171, 119],
    },
    Species {
        id: 11,
        name: "Broadleaf herb",
        files: "HTML Vue Svelte templates",
        color: [118, 150, 102],
    },
    Species {
        id: 12,
        name: "Wild rose",
        files: "Elixir .ex .exs",
        color: [192, 146, 141],
    },
    Species {
        id: 13,
        name: "Water lily",
        files: "Images fonts audio video",
        color: [215, 220, 193],
    },
    Species {
        id: 14,
        name: "Mossy stone",
        files: "SQL Prisma",
        color: [145, 158, 115],
    },
    Species {
        id: 15,
        name: "Clover",
        files: "Other files",
        color: [136, 163, 111],
    },
    Species {
        id: 16,
        name: "Bamboo",
        files: "Go .go",
        color: [143, 165, 96],
    },
    Species {
        id: 17,
        name: "Prickly pear",
        files: "C C++ .c .h .cpp",
        color: [144, 164, 119],
    },
    Species {
        id: 18,
        name: "Tulips",
        files: "Java Kotlin .java .kt",
        color: [188, 121, 108],
    },
    Species {
        id: 19,
        name: "Apple sapling",
        files: "Swift Objective-C .swift .m",
        color: [155, 167, 101],
    },
    Species {
        id: 20,
        name: "Poppies",
        files: "Ruby .rb",
        color: [190, 116, 96],
    },
    Species {
        id: 21,
        name: "Iris",
        files: "PHP .php",
        color: [153, 140, 168],
    },
    Species {
        id: 22,
        name: "Heather",
        files: "Haskell Erlang .hs .erl",
        color: [168, 146, 160],
    },
    Species {
        id: 23,
        name: "Violets",
        files: "Lua .lua",
        color: [144, 138, 163],
    },
    Species {
        id: 24,
        name: "Cattails",
        files: "Shell .sh .bash .zsh",
        color: [142, 126, 91],
    },
    Species {
        id: 25,
        name: "Rushes",
        files: "YAML .yaml .yml",
        color: [141, 164, 104],
    },
    Species {
        id: 26,
        name: "Maidenhair fern",
        files: "JSON .json .jsonc",
        color: [150, 174, 121],
    },
    Species {
        id: 27,
        name: "Sedum",
        files: "TOML .toml",
        color: [182, 150, 147],
    },
    Species {
        id: 28,
        name: "Pampas grass",
        files: "Data .csv .tsv .parquet",
        color: [202, 194, 153],
    },
    Species {
        id: 29,
        name: "Blueberry bush",
        files: "GraphQL .graphql .gql",
        color: [128, 146, 167],
    },
    Species {
        id: 30,
        name: "Chamomile",
        files: "Sass Less .scss .sass .less",
        color: [221, 217, 181],
    },
    Species {
        id: 31,
        name: "Nasturtium",
        files: "R Julia .r .jl",
        color: [200, 146, 93],
    },
];

pub const SHORE_ASSETS: [&str; 16] = [
    "Rounded stones",
    "Reeds",
    "Lily pads",
    "Pond flowers",
    "Driftwood",
    "Stepping stones",
    "Grass tuft",
    "Duck",
    "Ducklings",
    "Submerged log",
    "Wooden landing",
    "Cattails",
    "Pebbles",
    "Arrowhead leaves",
    "Snail",
    "Submerged stone",
];

pub fn classify(path: &str) -> Species {
    let lower = path.to_ascii_lowercase();
    let name = Path::new(&lower)
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let ext = Path::new(&lower)
        .extension()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let test = lower
        .split('/')
        .any(|part| matches!(part, "test" | "tests" | "__tests__" | "spec" | "specs"))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || name.ends_with("_test.go");
    let id = if name.ends_with(".lock")
        || matches!(
            name,
            "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" | "go.sum"
        ) {
        9
    } else if test {
        6
    } else if matches!(name, "dockerfile" | "makefile" | "justfile")
        || name.starts_with("dockerfile.")
    {
        10
    } else if name.starts_with('.') {
        8
    } else {
        match ext {
            "rs" => 0,
            "ts" | "tsx" | "mts" | "cts" => 1,
            "js" | "jsx" | "mjs" | "cjs" => 2,
            "py" | "pyi" | "ipynb" => 3,
            "css" => 4,
            "cs" | "fs" | "fsi" | "ml" | "mli" => 5,
            "md" | "mdx" | "rst" | "txt" | "adoc" => 7,
            "ini" | "conf" | "env" | "xml" => 8,
            "html" | "htm" | "vue" | "svelte" | "astro" => 11,
            "ex" | "exs" => 12,
            "png" | "jpg" | "jpeg" | "svg" | "webp" | "gif" | "ico" | "ttf" | "woff" | "woff2"
            | "mp3" | "wav" | "mp4" => 13,
            "sql" | "prisma" => 14,
            "go" => 16,
            "c" | "h" | "cpp" | "hpp" | "cc" => 17,
            "java" | "kt" | "kts" => 18,
            "swift" | "m" | "mm" => 19,
            "rb" | "rake" => 20,
            "php" => 21,
            "hs" | "lhs" | "erl" | "hrl" => 22,
            "lua" => 23,
            "sh" | "bash" | "zsh" | "fish" | "ps1" => 24,
            "yaml" | "yml" => 25,
            "json" | "jsonc" => 26,
            "toml" => 27,
            "csv" | "tsv" | "parquet" | "arrow" => 28,
            "graphql" | "gql" => 29,
            "scss" | "sass" | "less" => 30,
            "r" | "jl" => 31,
            _ => 15,
        }
    };
    SPECIES[id]
}

/// Stable across platforms, processes, and Rust versions.
pub fn hash(text: &str) -> u64 {
    text.as_bytes().iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_and_extensions() {
        for (path, id) in [
            ("src/lib.rs", 0),
            ("app.tsx", 1),
            ("tests/config.json", 6),
            ("pnpm-lock.yaml", 9),
            ("Cargo.toml", 27),
            (".github/workflows/ci.yml", 25),
            ("Dockerfile", 10),
            ("src/x_test.go", 6),
            ("data/schema.sql", 14),
            ("LICENSE", 15),
            ("world.go", 16),
            ("leaf.c", 17),
            ("garden.rb", 20),
            ("config.json", 26),
            ("petals.scss", 30),
            ("study.R", 31),
        ] {
            assert_eq!(classify(path).id, id, "{path}");
        }
    }
}
