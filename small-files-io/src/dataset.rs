use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub enum Layout {
    Sharded,
    Flat,
}

impl Layout {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "sharded" => Ok(Self::Sharded),
            "flat" => Ok(Self::Flat),
            _ => Err(format!("unknown layout: {value}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sharded => "sharded",
            Self::Flat => "flat",
        }
    }
}

pub struct GenerateConfig {
    pub count: usize,
    pub target_size: usize,
    pub output_dir: PathBuf,
    pub seed: u64,
    pub layout: Layout,
    pub overwrite: bool,
}

pub struct GenerateSummary {
    pub output_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub files: usize,
    pub target_size: usize,
    pub actual_bytes: u64,
    pub layout: Layout,
}

pub fn generate(config: &GenerateConfig) -> io::Result<GenerateSummary> {
    prepare_output_dir(config)?;

    let files_dir = config.output_dir.join("files");
    fs::create_dir_all(&files_dir)?;

    let mut actual_bytes = 0u64;

    for id in 0..config.count {
        let relative_path = document_relative_path(id, config.layout);
        let path = files_dir.join(&relative_path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let markdown = build_markdown(id, config.target_size, config.seed);
        actual_bytes += markdown.len() as u64;
        fs::write(path, markdown)?;
    }

    let manifest_path = config.output_dir.join("manifest.json");
    fs::write(&manifest_path, build_manifest(config, actual_bytes))?;

    Ok(GenerateSummary {
        output_dir: config.output_dir.clone(),
        manifest_path,
        files: config.count,
        target_size: config.target_size,
        actual_bytes,
        layout: config.layout,
    })
}

fn prepare_output_dir(config: &GenerateConfig) -> io::Result<()> {
    if config.output_dir.exists() {
        if config.overwrite {
            fs::remove_dir_all(&config.output_dir)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{} already exists; pass --overwrite to replace it",
                    config.output_dir.display()
                ),
            ));
        }
    }

    fs::create_dir_all(&config.output_dir)
}

fn document_relative_path(id: usize, layout: Layout) -> PathBuf {
    let file_name = format!("doc-{id:08}.md");

    match layout {
        Layout::Flat => PathBuf::from(file_name),
        Layout::Sharded => {
            let shard = id % 256;
            Path::new(&format!("{shard:02x}")).join(file_name)
        }
    }
}

fn build_markdown(id: usize, target_size: usize, seed: u64) -> String {
    let created_day = (id % 28) + 1;
    let updated_day = ((id * 7) % 28) + 1;
    let tag_a = TAGS[id % TAGS.len()];
    let tag_b = TAGS[(id / TAGS.len() + 3) % TAGS.len()];
    let title = format!("Document {id:08}");
    let slug = format!("doc-{id:08}");

    let mut markdown = format!(
        "---\n\
title: {title}\n\
tags: [{tag_a}, {tag_b}, generated]\n\
created_at: 2026-01-{created_day:02}T00:00:00Z\n\
updated_at: 2026-02-{updated_day:02}T00:00:00Z\n\
---\n\n\
# {title}\n\n\
This is a deterministic Markdown document for SQLite and file-system I/O benchmarking.\n\
It includes frontmatter, headings, lists, code blocks, and internal links.\n\n\
## Notes\n\n\
- primary tag: {tag_a}\n\
- secondary tag: {tag_b}\n\
- link: [[{slug}]]\n\n\
```text\n\
document_id={id}\n\
seed={seed}\n\
```\n\n"
    );

    let mut state = seed ^ ((id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut paragraph_index = 0usize;

    while markdown.len() < target_size {
        state = next_state(state);
        let word_a = WORDS[(state as usize) % WORDS.len()];
        state = next_state(state);
        let word_b = WORDS[(state as usize) % WORDS.len()];
        state = next_state(state);
        let word_c = WORDS[(state as usize) % WORDS.len()];

        markdown.push_str(&format!(
            "Paragraph {paragraph_index}: {word_a} {word_b} {word_c} content keeps the byte size realistic while remaining repeatable for benchmark runs.\n\n"
        ));
        paragraph_index += 1;
    }

    markdown.truncate(target_size);
    markdown
}

fn next_state(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

fn build_manifest(config: &GenerateConfig, actual_bytes: u64) -> String {
    format!(
        concat!(
            "{{\n",
            "  \"count\": {},\n",
            "  \"target_size\": {},\n",
            "  \"actual_bytes\": {},\n",
            "  \"seed\": {},\n",
            "  \"layout\": \"{}\",\n",
            "  \"files_dir\": \"files\"\n",
            "}}\n"
        ),
        config.count,
        config.target_size,
        actual_bytes,
        config.seed,
        config.layout.as_str()
    )
}

const TAGS: &[&str] = &[
    "sqlite",
    "filesystem",
    "markdown",
    "benchmark",
    "note",
    "search",
    "cache",
    "storage",
];

const WORDS: &[&str] = &[
    "alpha",
    "bravo",
    "charlie",
    "delta",
    "echo",
    "foxtrot",
    "golf",
    "hotel",
    "index",
    "journal",
    "kernel",
    "latency",
    "metadata",
    "normal",
    "offset",
    "page",
    "query",
    "record",
    "sync",
    "throughput",
    "update",
    "vacuum",
    "write",
    "xattr",
];
