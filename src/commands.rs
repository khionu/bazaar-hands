use std::sync::LazyLock;
// Basic utilities
use snafu::{whatever, ResultExt, Whatever};
// Discord
use twilight_model::{
  application::{
    command::{Command,CommandType},
    interaction::{Interaction, InteractionData, InteractionType, InteractionContextType},
  },
  http::interaction::InteractionResponse,
};
use twilight_model::application::command::{CommandOption, CommandOptionChoice, CommandOptionChoiceValue, CommandOptionType};
use twilight_util::builder::command::{CommandBuilder, StringBuilder, SubCommandBuilder, UserBuilder};

mod ping;

pub static CMDS: LazyLock<&[Command]> = LazyLock::new(|| {
  let x = Box::new([
    CommandBuilder::new("ping", "Round-trip latency checker", CommandType::ChatInput)
      .build()
  ]);

  Box::leak(x) as &'static [Command]
});

pub async fn handle(payload: Interaction) -> Result<InteractionResponse, Whatever> {
  match payload.kind {
    InteractionType::ApplicationCommand => {
      let InteractionData::ApplicationCommand(cmd) =
        payload.data.expect("missing data field on app cmd") else {
          unreachable!("data is not for an app cmd");
      };

      match cmd.kind {
        CommandType::ChatInput => {
          match cmd.name.as_str() {
            "ping" => ping::cmd(payload.token).await,
            _ => whatever!("unexpected command"),
          }
        }
        _ => whatever!("unexpected command type"),
      }
    }
    _ => whatever!("Unexpected interaction type: {:?}", payload.kind),
  }
}
