use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use cw_core::APP_ID;

const OBJECT_PATH: &str = "/io/github/vynxc/CosmicWidgets";
const CONTROL_INTERFACE: &str = "io.github.vynxc.CosmicWidgets.Control1";

#[derive(Debug, Parser)]
#[command(version, about = "Small COSMIC panel controller for cosmic-widgets")]
struct Cli {
    #[arg(value_enum, default_value_t = Action::ToggleEdit)]
    action: Action,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Action {
    ToggleEdit,
    Show,
    Hide,
    Status,
}

fn main() -> Result<()> {
    let connection = zbus::blocking::Connection::session()
        .context("cosmic-widgets is not reachable on the session bus")?;
    let proxy = zbus::blocking::Proxy::new(&connection, APP_ID, OBJECT_PATH, CONTROL_INTERFACE)?;
    match Cli::parse().action {
        Action::ToggleEdit => {
            let current: bool = proxy.get_property("EditMode")?;
            proxy.set_property("EditMode", !current)?;
        }
        Action::Show => proxy.set_property("Visible", true)?,
        Action::Hide => proxy.set_property("Visible", false)?,
        Action::Status => {
            let edit_mode: bool = proxy.get_property("EditMode")?;
            let visible: bool = proxy.get_property("Visible")?;
            println!("visible={visible} edit_mode={edit_mode}");
        }
    }
    Ok(())
}
