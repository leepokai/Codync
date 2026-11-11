//! The `/n8n …` and `/comfy …` commands of the chat box: a message that starts with one asks the bot to do
//! its request with that tool. Like a skill's command, the host puts a note before the text; the bot works
//! n8n through the connector the host registers while the instance runs, and makes images with the
//! `codync-comfyui` tools. A skill with the same id wins (`market::expand_command` goes first).

use crate::market::slash_command;

/// A command that hands the request to one of the computer's tools: what follows the `/`, and the note.
struct ToolCommand {
    name: &'static str,
    note: &'static str,
}

const COMMANDS: [ToolCommand; 2] = [
    ToolCommand {
        name: "n8n",
        note: "[Codync: the user asks you to do this with n8n, the workflow tool on this computer. Use the n8n \
               connector's tools: look at the workflows, run one or follow its executions, as the request needs. \
               If the connector isn't there, n8n isn't running: say so and tell them to start it in Settings → n8n.]",
    },
    ToolCommand {
        name: "comfy",
        note: "[Codync: the user asks you to make this with ComfyUI, the image tool on this computer. Use the \
               codync-comfyui tools: list_models first if you need a checkpoint, then queue the image and give the \
               result back. If the tools aren't there, ComfyUI isn't running: say so and tell them to start it from \
               its page in the sidebar.]",
    },
];

/// The commands the clients list after the skills: `(name, title, detail)`; the clients translate the words.
pub const LISTED: [(&str, &str, &str); 2] =
    [("n8n", "n8n", "Do it with the computer's n8n workflows"), ("comfy", "ComfyUI", "Make an image with ComfyUI")];

/// `text` with the tool's note before the request, when it starts with `/n8n` or `/comfy`; `None` sends the
/// message as written.
pub fn expand(text: &str) -> Option<String> {
    let (name, rest) = slash_command(text)?;
    let command = COMMANDS.iter().find(|c| c.name == name)?;
    Some(match rest.map(str::trim).filter(|r| !r.is_empty()) {
        Some(task) => format!("{}\n\n{task}", command.note),
        None => command.note.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_command_puts_its_note_before_the_request() {
        let asked = expand("/n8n run the daily report").expect("n8n command");
        assert!(asked.starts_with("[Codync:") && asked.contains("n8n connector"));
        assert!(asked.ends_with("\n\nrun the daily report"));
        let image = expand("/comfy a red fox\nin the snow").expect("comfy command");
        assert!(image.contains("codync-comfyui") && image.ends_with("a red fox\nin the snow"));
    }

    #[test]
    fn a_command_alone_is_just_its_note_and_other_text_is_left_alone() {
        assert!(expand("/n8n").expect("bare command").ends_with(']'));
        assert!(expand("/comfy   ").expect("blank request").ends_with(']'));
        assert_eq!(expand("hello /n8n"), None);
        assert_eq!(expand("/fast"), None);
        assert_eq!(expand("/n8nx do it"), None, "only the exact name");
    }

    #[test]
    fn every_listed_command_expands() {
        for (name, ..) in LISTED {
            assert!(expand(&format!("/{name} go")).is_some(), "{name}");
        }
    }
}
