//! Scout commands and callbacks — merged into the main TelegramBot.

pub(crate) mod act;
pub mod callbacks;
pub mod commands;
pub mod formatting;
pub(crate) mod helpers;
pub mod scout_commands;

#[cfg(test)]
mod tests {
    use crate::message_helpers::parse_command;

    #[test]
    fn parse_command_simple() {
        let (cmd, args) = parse_command("/addlink https://example.com");
        assert_eq!(cmd, "addlink");
        assert_eq!(args, "https://example.com");
    }

    #[test]
    fn parse_command_no_args() {
        let (cmd, args) = parse_command("/scout");
        assert_eq!(cmd, "scout");
        assert_eq!(args, "");
    }

    #[test]
    fn parse_command_with_bot_mention() {
        let (cmd, args) = parse_command("/list@scout_bot saved");
        assert_eq!(cmd, "list");
        assert_eq!(args, "saved");
    }
}
