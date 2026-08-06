use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use cw_core::{APP_ID, CosmicTheme, Layout, PackageLimits, validate_package};
use tracing_subscriber::EnvFilter;

const OBJECT_PATH: &str = "/io/github/vynxc/CosmicWidgets";

#[derive(Debug, Parser)]
#[command(
    name = "cosmic-widgets",
    version,
    about = "Native HTML/CSS widgets for COSMIC"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the desktop widget host and D-Bus service.
    Serve,
    /// Validate a local .cwidget package without installing it.
    Validate { package: PathBuf },
    /// Atomically install a validated local package.
    Install { package: PathBuf },
    /// List locally installed packages.
    List,
    /// Print Wayland, COSMIC, and storage diagnostics.
    Doctor,
    /// Print the fallback COSMIC theme bridge stylesheet.
    ThemeCss,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match Cli::parse().command {
        Command::Serve => serve(),
        Command::Validate { package } => validate(&package),
        Command::Install { package } => install(&package),
        Command::List => list(),
        Command::Doctor => doctor(),
        Command::ThemeCss => {
            println!("{}", CosmicTheme::default().to_css());
            Ok(())
        }
    }
}

fn validate(path: &Path) -> Result<()> {
    let package = validate_package(path, PackageLimits::default())
        .with_context(|| format!("package validation failed for {}", path.display()))?;
    println!(
        "{} {}\n{} files\nsha256 {}",
        package.manifest.id,
        package.manifest.version,
        package.files.len(),
        package.sha256
    );
    Ok(())
}

fn install(path: &Path) -> Result<()> {
    let package = validate_package(path, PackageLimits::default())
        .with_context(|| format!("package validation failed for {}", path.display()))?;
    let root = package_root()?;
    let directory = root.join(&package.manifest.id);
    fs::create_dir_all(&directory)
        .with_context(|| format!("unable to create {}", directory.display()))?;
    let filename = format!("{}.cwidget", package.manifest.version);
    let destination = directory.join(filename);
    let temporary = directory.join(format!(".{}.tmp", package.manifest.version));
    fs::copy(path, &temporary)
        .with_context(|| format!("unable to stage {}", temporary.display()))?;
    fs::rename(&temporary, &destination)
        .with_context(|| format!("unable to activate {}", destination.display()))?;
    println!(
        "Installed {} {}",
        package.manifest.id, package.manifest.version
    );
    Ok(())
}

fn list() -> Result<()> {
    let root = package_root()?;
    if !root.exists() {
        println!("No widgets installed.");
        return Ok(());
    }
    let mut installed = Vec::new();
    for package in
        fs::read_dir(&root).with_context(|| format!("unable to read {}", root.display()))?
    {
        let package = package?;
        if !package.file_type()?.is_dir() {
            continue;
        }
        for version in fs::read_dir(package.path())? {
            let version = version?;
            if version
                .path()
                .extension()
                .is_some_and(|extension| extension == "cwidget")
            {
                installed.push(version.path());
            }
        }
    }
    installed.sort();
    for path in installed {
        println!("{}", path.display());
    }
    Ok(())
}

fn doctor() -> Result<()> {
    println!("application: {APP_ID}");
    println!("packages: {}", package_root()?.display());
    match cw_shell_wayland::probe_session() {
        Ok(probe) => {
            println!("wayland: connected ({})", probe.display);
            println!("desktop: {}", probe.desktop.as_deref().unwrap_or("unknown"));
        }
        Err(error) => println!("wayland: unavailable ({error})"),
    }
    println!("renderer: Blitz DOM + direct AnyRender/Vello/WGPU presentation");
    println!("widget logic: Extism, lazy, WASI disabled");
    Ok(())
}

fn package_root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path).join("cosmic-widgets/widgets"));
    }
    let home = std::env::var_os("HOME").context("HOME and XDG_DATA_HOME are both unset")?;
    Ok(PathBuf::from(home).join(".local/share/cosmic-widgets/widgets"))
}

#[derive(Clone, Default)]
struct ControlService {
    state: Arc<ControlState>,
}

#[derive(Default)]
struct ControlState {
    edit_mode: AtomicBool,
    visible: AtomicBool,
    layout: RwLock<Layout>,
}

#[zbus::interface(name = "io.github.vynxc.CosmicWidgets.Control1")]
impl ControlService {
    #[expect(
        clippy::unused_self,
        reason = "D-Bus interface methods require a service receiver"
    )]
    fn ping(&self) -> &'static str {
        "cosmic-widgets/1"
    }

    #[zbus(property)]
    fn edit_mode(&self) -> bool {
        self.state.edit_mode.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn set_edit_mode(&self, value: bool) {
        self.state.edit_mode.store(value, Ordering::Relaxed);
    }

    #[zbus(property)]
    fn visible(&self) -> bool {
        self.state.visible.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn set_visible(&self, value: bool) {
        self.state.visible.store(value, Ordering::Relaxed);
    }

    fn layout_json(&self) -> zbus::fdo::Result<String> {
        let layout = self
            .state
            .layout
            .read()
            .map_err(|_| zbus::fdo::Error::Failed("layout lock was poisoned".into()))?;
        serde_json::to_string(&*layout).map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }
}

fn serve() -> Result<()> {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        bail!("WAYLAND_DISPLAY is not set; cosmic-widgets must run inside a Wayland session");
    }
    let service = ControlService::default();
    service.state.visible.store(true, Ordering::Relaxed);
    let html_provider: cw_shell_wayland::HtmlProvider = Arc::new(clock_document);
    let _widget_thread = std::thread::Builder::new()
        .name("cosmic-widget-clock".into())
        .spawn(move || {
            if let Err(error) = cw_shell_wayland::run_desktop_widget(
                cw_shell_wayland::DesktopWidgetConfig::default(),
                html_provider,
            ) {
                tracing::error!(%error, "desktop widget host stopped");
            }
        })
        .context("unable to start desktop widget thread")?;
    let _connection = zbus::blocking::connection::Builder::session()?
        .name(APP_ID)?
        .serve_at(OBJECT_PATH, service)?
        .build()?;
    tracing::info!(service = APP_ID, "widget host is ready");
    loop {
        std::thread::park();
    }
}

fn clock_document() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let mut html = include_str!("../../../widgets/clock/index.html").to_owned();
    set_bound_text(&mut html, "clock.weekday", &format!("{:?}", now.weekday()));
    set_bound_text(
        &mut html,
        "clock.time",
        &format!("{:02}:{:02}", now.hour(), now.minute()),
    );
    set_bound_text(
        &mut html,
        "clock.date",
        &format!("{:?} {}, {}", now.month(), now.day(), now.year()),
    );
    format!(
        "<!doctype html><html><head><style>{}\nhtml,body{{margin:0;width:100%;height:100%;background:transparent}}\n{}</style></head><body>{html}</body></html>",
        CosmicTheme::default().to_css(),
        include_str!("../../../widgets/clock/style.css"),
    )
}

fn set_bound_text(html: &mut String, path: &str, value: &str) {
    let marker = format!("data-cw-text=\"{path}\">");
    let Some(start) = html.find(&marker).map(|position| position + marker.len()) else {
        return;
    };
    let Some(end) = html[start..].find('<').map(|position| start + position) else {
        return;
    };
    html.replace_range(start..end, value);
}
