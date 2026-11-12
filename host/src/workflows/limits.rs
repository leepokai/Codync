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
/// JavaScript loop iterations in one replay. Native calls are not counted here.
pub const SCRIPT_LOOP_LIMIT: u64 = 10_000;
/// Nested calls in one replay, including the host's own depth walk.
pub const SCRIPT_RECURSION_LIMIT: usize = 64;
/// Boa's operand stack, not the operating-system stack.
pub const SCRIPT_STACK_LIMIT: usize = 1_024;
/// Wall clock for one replay. The script runs in its own process and is killed after this.
pub const SCRIPT_WALL: std::time::Duration = std::time::Duration::from_secs(2);
/// Reply bytes read from that process. Past this, the rest is discarded and the replay fails.
pub const SCRIPT_REPLY_CAP: usize = 2 * 1024 * 1024;
/// Extra memory a script may use past what its process needed to start.
pub const SCRIPT_MEMORY: u64 = 128 * 1024 * 1024;

/// Address space (Unix) or committed memory (Windows) the script process may reach.
/// `current` is what the process already uses. The script may take [`SCRIPT_MEMORY`] more.
pub fn script_memory_limit(current: u64) -> u64 {
    current.saturating_add(SCRIPT_MEMORY)
}

/// Object and array nesting accepted from a script value.
pub const SCRIPT_JSON_DEPTH: usize = 32;
/// Parser nesting. Deeper source is rejected before the recursive parser runs.
pub const SCRIPT_PARSE_DEPTH: usize = 64;
pub const MAX_ERROR_CHARS: usize = 500;

pub const NOTE_TONES: [&str; 6] = ["yellow", "green", "blue", "violet", "orange", "gray"];
pub const SHAPES: [&str; 4] = ["rectangle", "ellipse", "diamond", "triangle"];
pub const ALIGNS: [&str; 3] = ["leading", "center", "trailing"];

#[cfg(test)]
mod tests {
    use super::{SCRIPT_MEMORY, script_memory_limit};

    #[test]
    fn a_script_keeps_its_allowance_above_the_process() {
        assert_eq!(script_memory_limit(0), SCRIPT_MEMORY);
        assert_eq!(script_memory_limit(500 * 1024 * 1024), 500 * 1024 * 1024 + SCRIPT_MEMORY);
        assert_eq!(script_memory_limit(u64::MAX), u64::MAX);
    }
}
