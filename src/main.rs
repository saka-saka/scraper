mod application;
mod bigweb_scraper;
mod domain;
mod error;
mod limitless_scraper;
mod one_piece_csv;
mod one_piece_scraper;
mod pokemon_csv;
mod pokemon_trainer_scraper;
mod repository;
mod scraper_error;
mod ws_csv;
mod ws_scraper;
mod yugioh_csv;
mod yugioh_scraper;

use application::Application;
use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use dotenvy::dotenv;
use pokemon_csv::PokemonCSV;
use std::{thread::sleep, time::Duration};
use tracing::Level;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    UpdateCardset {
        #[arg(short, long)]
        all: bool,
    },
    UpdateCard {
        #[arg(short, long)]
        all: Option<bool>,
        #[arg(short, long)]
        set: Option<String>,
    },
    DownloadImage,
    ExportCard {
        #[arg(short, long)]
        all: bool,
    },
    #[command(subcommand)]
    PokemonTrainer(PokemonTrainerCommands),
    #[command(subcommand)]
    Yugioh(YugiohCommands),
    #[command(subcommand)]
    Ws(WsCommands),
    #[command(subcommand)]
    OnePiece(OnePieceCommands),
    #[command(subcommand)]
    Limitless(LimitlessCommands),
    ResyncAll,
}

#[derive(Subcommand)]
enum PokemonTrainerCommands {
    Prepare,
    Run,
    ExportCsv,
    List,
}

#[derive(Subcommand)]
enum WsCommands {
    Scrape,
    ExportCsv,
}

#[derive(Subcommand)]
enum YugiohCommands {
    BuildExpLink,
    BuildPriLink,
    BuildDetail,
    ExportCsv,
}

#[derive(Subcommand)]
enum OnePieceCommands {
    Scrape,
    ScrapeProducts,
    DownloadImages,
    ExportCsv,
    ExportProductCsv,
}

#[derive(Subcommand)]
enum LimitlessCommands {
    Poc,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    dotenv()?;
    let cli = Cli::parse();
    let database_url = std::env::var("DATABASE_URL")?;
    let application = Application::new(&database_url);
    tracing_subscriber::fmt()
        .with_max_level(Level::ERROR)
        .finish();

    match &cli.command {
        Some(Commands::UpdateCard { all, set }) => {
            if let Some(all) = all {
                if *all {
                    application.update_entire_card_db().await?;
                }
            } else if let Some(set) = set {
                if !set.is_empty() {
                    application.update_single_set_card_db(set).await?;
                }
            }
        }
        Some(Commands::UpdateCardset { all }) => {
            if *all {
                application.update_entire_cardset_db().await?;
            }
        }
        Some(Commands::ExportCard { all }) => {
            if *all {
                let all_cards = application.export_entire_card_db().await?;
                let mut wtr = csv::Writer::from_writer(std::io::stdout());
                for card in all_cards {
                    let csv_card: PokemonCSV = card.into();
                    wtr.serialize(csv_card)?;
                }
                wtr.flush()?;
            }
        }
        Some(Commands::PokemonTrainer(commands)) => match commands {
            PokemonTrainerCommands::Prepare => {
                let pokemon_trainer = application.pokemon_trainer();
                pokemon_trainer
                    .update_entire_pokemon_trainer_expansion()
                    .await?;
                pokemon_trainer.build_pokemon_trainer_fetchable().await?;
                pokemon_trainer.update_pokemon_trainer_printing().await?;
                pokemon_trainer.download_all_image().await?;
            }
            PokemonTrainerCommands::Run => {
                let pokemon_trainer = application.pokemon_trainer();
                pokemon_trainer.update_rarity().await?;
            }
            PokemonTrainerCommands::ExportCsv => {
                let pokemon_trainer = application.pokemon_trainer();
                pokemon_trainer.export_pokemon_trainer().await?;
            }
            PokemonTrainerCommands::List => {
                let expansions = application.pokemon_trainer().list_all_expansions().await?;
                println!("{expansions:#?}");
            }
        },
        Some(Commands::DownloadImage) => {
            application.download_image().await.unwrap();
        }
        Some(Commands::ResyncAll) => {
            application.unsync_entire_cardset_db().await?;
            application.update_entire_card_db().await?;
        }
        Some(Commands::Yugioh(YugiohCommands::BuildExpLink)) => {
            application.yugioh().build_yugioh_expansion_link().await;
        }
        Some(Commands::Yugioh(YugiohCommands::BuildPriLink)) => loop {
            application
                .yugioh()
                .build_yugioh_printing_link()
                .await
                .expect("ran out of expansion link to build printing link");
            println!("done wait for 1 secs ...");
            sleep(Duration::from_secs(1));
        },
        Some(Commands::Yugioh(YugiohCommands::BuildDetail)) => loop {
            let waiting_secs = 1;
            application
                .yugioh()
                .build_yugioh_printing_detail()
                .await
                .expect("ran out of printing link");
            println!("done wait for {waiting_secs} secs ...");
            sleep(Duration::from_secs(waiting_secs));
        },
        Some(Commands::Yugioh(YugiohCommands::ExportCsv)) => {
            let wtr = std::io::stdout();
            application
                .yugioh()
                .export_yugioh_printing_detail(wtr)
                .await;
        }
        Some(Commands::Ws(WsCommands::Scrape)) => {
            application.scrape_ws().await;
        }
        Some(Commands::Ws(WsCommands::ExportCsv)) => {
            application.export_ws_csv(std::io::stdout()).await;
        }
        Some(Commands::OnePiece(OnePieceCommands::Scrape)) => {
            application.one_piece().scrape_one_piece().await?;
        }
        Some(Commands::OnePiece(OnePieceCommands::DownloadImages)) => {
            application.one_piece().download_images().await?;
        }
        Some(Commands::OnePiece(OnePieceCommands::ScrapeProducts)) => {
            application.one_piece().scrape_one_piece_products().await;
        }
        Some(Commands::OnePiece(OnePieceCommands::ExportCsv)) => {
            let wtr = std::io::stdout();
            application.one_piece().export_one_piece_csv(wtr).await?;
        }
        Some(Commands::OnePiece(OnePieceCommands::ExportProductCsv)) => {
            let wtr = std::io::stdout();
            application
                .one_piece()
                .export_one_piece_product_csv(wtr)
                .await?;
        }
        Some(Commands::Limitless(LimitlessCommands::Poc)) => {
            application.poc().await;
        }
        None => {}
    }
    Ok(())
}
