mod activity;
mod app;
mod config;
mod display;
mod emoji;
mod entry;
mod home_assistant;
mod mediamtx;
mod power;
mod schedule;
mod slack;
mod web;
mod youtube;

use std::io::IsTerminal;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    app::run(Config::parse()).await
}
