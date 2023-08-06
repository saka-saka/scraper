use std::io::Write;

use crate::bigweb_scraper::BigwebScraper;
use crate::domain::{CardsetURL, PokemonCard, Rarity};
use crate::pokemon_trainer_scraper::{PokemonTrainerSiteScraper, ThePTCGSet};
use crate::repository::Repository;
use strum::IntoEnumIterator;
use tracing::{debug, error};

pub struct Application {
    the_ptcg_scraper: PokemonTrainerSiteScraper,
    bigweb_scraper: BigwebScraper,
    repository: Repository,
}

impl Application {
    pub async fn new(url: &str) -> Self {
        let bigweb_scraper = BigwebScraper::new().unwrap();
        let the_ptcg_scraper = PokemonTrainerSiteScraper::new().await.unwrap();
        let bigweb_repository = Repository::new(url);
        Self {
            the_ptcg_scraper,
            bigweb_scraper,
            repository: bigweb_repository,
        }
    }
    pub async fn download_image(&self) -> Result<(), Error> {
        let all_cards = self.repository.fetch_card_ids().await.unwrap();
        let client = reqwest::Client::new();
        for card in all_cards {
            let image_url = self.bigweb_scraper.fetch_pokemon_card_image(&card).unwrap();
            let result = match client.get(&image_url).send().await {
                Ok(r) => r,
                Err(_) => continue,
            };
            let paths = result.url().path_segments().unwrap();
            let filename = paths.last().unwrap();
            let save_path = format!("images/{}", filename);
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(save_path)
                .unwrap();
            let bytes = result.bytes().await.unwrap();
            file.write(&bytes).unwrap();
            self.repository.image_downloaded(&card).await.unwrap();
        }
        Ok(())
    }
    pub async fn update_entire_cardset_db(&self) -> Result<(), Error> {
        let pokemon_cardsets = &self.bigweb_scraper.fetch_pokemon_cardset()?;
        let (sets, errs): (Vec<_>, Vec<_>) =
            pokemon_cardsets
                .iter()
                .fold((vec![], vec![]), |mut acc, elem| {
                    match elem {
                        Ok(result) => acc.0.push(result),
                        Err(err) => acc.1.push(err),
                    };
                    acc
                });
        for err in errs {
            error!(?err)
        }
        for cardset in sets {
            debug!(?cardset);
            self.repository.upsert_cardset(cardset).await?;
        }
        Ok(())
    }
    pub async fn update_single_set_card_db(&self, set_ref: &str) -> Result<(), Error> {
        let set_id = self
            .repository
            .get_cardset_id(set_ref)
            .await?
            .ok_or(Error::SetNotExists(set_ref.to_string()))?;
        self.update_whole_set_card_db(&set_id).await?;
        Ok(())
    }
    async fn update_whole_set_card_db(&self, set_id: &str) -> Result<(), Error> {
        let cardset_url = CardsetURL::from_cardset_id(set_id).unwrap();
        let cards = self
            .bigweb_scraper
            .fetch_pokemon_data(cardset_url.origin_url().as_str())?;

        let (cards, errs): (Vec<_>, Vec<_>) =
            cards.iter().fold((vec![], vec![]), |mut acc, elem| {
                match elem {
                    Ok(value) => acc.0.push(value),
                    Err(err) => acc.1.push(err),
                };
                acc
            });
        for card in cards {
            self.repository.upsert_card(card).await?;
        }
        if !errs.is_empty() {
            for err in errs {
                error!(?err);
            }
        } else {
            self.repository.synced(set_id).await?;
        }
        Ok(())
    }
    pub async fn update_entire_card_db(&self) -> Result<(), Error> {
        let cardset_ids = self.repository.get_cardset_ids(false).await?;
        for set_id in cardset_ids {
            match self.update_whole_set_card_db(&set_id).await {
                Ok(_) => {}
                Err(err) => {
                    error!(?err)
                }
            };
        }
        Ok(())
    }
    pub async fn export_entire_card_db(&self) -> Result<Vec<PokemonCard>, Error> {
        let all_cards = self.repository.fetch_all_cards().await?;
        Ok(all_cards)
    }
    pub async fn unsync_entire_cardset_db(&self) -> Result<(), Error> {
        let all_sets = self.repository.get_cardset_ids(true).await?;
        for set_id in all_sets {
            self.repository.unsync(&set_id).await?;
        }
        Ok(())
    }
    pub async fn list_all_expansions(&self) -> Result<Vec<ThePTCGSet>, Error> {
        let expansions = self.the_ptcg_scraper.fetch_expansion().await?;
        Ok(expansions)
    }
    pub async fn update_entire_pokemon_trainer_expansion(&self) {
        let expansions = self.the_ptcg_scraper.fetch_expansion().await.unwrap();
        for set in expansions {
            self.repository.upsert_pokemon_trainer_expansion(&set).await;
        }
    }
    pub async fn build_pokemon_trainer_fetchable(&self) -> Result<(), Error> {
        let expansion_codes = self
            .repository
            .get_all_pokemon_trainer_expansion_code()
            .await?;
        for expansion_code in expansion_codes {
            let fetchable_codes = self
                .the_ptcg_scraper
                .get_fetchables_by_set(&expansion_code)
                .await
                .unwrap();
            self.repository
                .upsert_fetchable(fetchable_codes, &expansion_code)
                .await;
        }
        Ok(())
    }
    pub async fn update_pokemon_trainer_printing(&self) {
        let fetchable_codes = self.repository.get_fetchable().await;
        for (code, set_code) in fetchable_codes {
            let mut card = match self
                .the_ptcg_scraper
                .fetch_printing_detail(&format!(
                    "https://asia.pokemon-card.com/tw/card-search/detail/{code}/"
                ))
                .await
            {
                Ok(card) => card,
                Err(err) => {
                    println!("{err} - {code}");
                    continue;
                }
            };
            card.set_code = Some(set_code);
            self.repository.upsert_the_ptcg_card(&card).await;
            self.repository.fetched(&code).await;
        }
    }
    pub async fn update_rarity(&self) {
        for rarity in Rarity::iter() {
            let ids = self.the_ptcg_scraper.rarity_ids(&rarity).await.unwrap();
            self.repository.update_the_ptcg_rarity(ids, &rarity).await;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bigweb scraper error")]
    Scraper(#[from] crate::scraper_error::Error),
    #[error("bigweb repository error")]
    Repository(#[from] crate::repository::Error),
    #[error("set is not exist {0}")]
    SetNotExists(String),
}
