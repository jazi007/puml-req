use anyhow::{anyhow, Context, Result};
use clap::Parser;
use log::{debug, info};
use plantuml_encoding::encode_plantuml_deflate;
use reqwest::{Client, Proxy};
use std::{fmt::Display, path::PathBuf, str::FromStr};

use tokio::{
    self, fs,
    io::{AsyncReadExt, AsyncWriteExt},
    task::JoinSet,
};

#[derive(Debug, Copy, Clone)]
enum Type {
    Ascii,
    Png,
    Svg,
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Type::Ascii => "txt",
                Type::Svg => "svg",
                Type::Png => "png",
            }
        )
    }
}

impl FromStr for Type {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ascii" | "txt" => Ok(Self::Ascii),
            "svg" => Ok(Self::Svg),
            "png" => Ok(Self::Png),
            _ => Err(format!("Unknown Type {s}")),
        }
    }
}

#[derive(Parser, Debug)]
struct Cli {
    /// Export Type
    #[arg(short, long = "type", default_value = "svg")]
    type_: Type,
    /// Plantuml server url
    #[arg(short, long, default_value = URL)]
    url: String,
    /// Glob paths
    path: Vec<PathBuf>,
}

fn make_output_path(input: PathBuf, type_: Type) -> Result<PathBuf> {
    let file_stem = input
        .file_stem()
        .context("no file stem for input path")?
        .to_str()
        .context("Cannot convert to str")?;
    let mut out_path = input.parent().context("no parent found")?.to_path_buf();
    out_path.push(PathBuf::from_str(file_stem)?.with_extension(format!("{type_}")));
    Ok(out_path)
}

const URL: &str = "http://www.plantuml.com/plantuml";

fn make_client() -> Result<Client> {
    match std::env::var("http_proxy") {
        Ok(proxy) => {
            debug!("Setting proxy to {proxy}");
            Ok(Client::builder().proxy(Proxy::http(proxy)?).build()?)
        }
        _ => Ok(Client::new()),
    }
}

async fn export(client: Client, path: PathBuf, url: String, type_: Type) -> Result<()> {
    info!("Processing {} ...", path.display());
    let mut uml = fs::OpenOptions::new().read(true).open(&path).await?;
    let mut uml_str = String::new();
    uml.read_to_string(&mut uml_str).await?;
    let encoded = encode_plantuml_deflate(uml_str).map_err(|e| anyhow!("{e:?}"))?;
    let url = format!("{}/{}/{encoded}", url, type_);
    let img = client.get(url).send().await?.bytes().await?;
    let out_path = make_output_path(path, type_)?;
    info!("Writting to {} ...", out_path.display());
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(out_path)
        .await?;
    out.write_all(&img).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let client = make_client()?;
    let mut set = JoinSet::new();
    for path in cli.path {
        set.spawn(export(client.clone(), path, cli.url.clone(), cli.type_));
    }
    while let Some(res) = set.join_next().await {
        res??;
    }
    Ok(())
}
