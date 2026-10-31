//! Caps shared by a save and a run.

pub const MAX_NODES: usize = 200;
pub const MAX_EDGES: usize = 400;
pub const MAX_STROKES: usize = 500;
pub const MAX_STROKE_POINTS: usize = 2000;
pub const MAX_FIELD_KEYS: usize = 32;
pub const MAX_KEY_LEN: usize = 64;
pub const MAX_FIELD_CHARS: usize = 20_000;
pub const MAX_TITLE: usize = 160;
pub const MAX_GRAPH_BYTES: usize = 1024 * 1024;
pub const MAX_COORD: f64 = 1_000_000.0;
pub const MAX_SIZE: f64 = 100_000.0;
pub const MAX_INPUT: usize = 32_000;
pub const MAX_TEXT: usize = 32_000;
pub const MAX_RUN_PAGE: i64 = 50;
pub const MAX_VERSIONS: i64 = 50;
pub const MAX_STEPS: i64 = 2000;
pub const MAX_PASSES: i64 = 20;
pub const VERSION_GAP_MS: i64 = 5 * 60 * 1000;
pub const STEP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3600);
pub const MAX_SCRIPT_BYTES: usize = 65_536;
pub const MAX_SCRIPT_CALLS: i64 = 100;
pub const MAX_PARALLEL: usize = 64;
pub const MAX_STAGES: usize = 8;
pub const MAX_PHASES: usize = 20;
pub const MAX_PHASE_TITLE: usize = 80;
pub const MAX_DESCRIPTION: usize = 2_000;
pub const SCRIPT_FUEL: usize = 100_000;
pub const MAX_ERROR_CHARS: usize = 500;

pub const NOTE_TONES: [&str; 6] = ["yellow", "green", "blue", "violet", "orange", "gray"];
pub const SHAPES: [&str; 4] = ["rectangle", "ellipse", "diamond", "triangle"];
pub const ALIGNS: [&str; 3] = ["leading", "center", "trailing"];
