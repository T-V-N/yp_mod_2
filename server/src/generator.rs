use crate::quote::StockQuote;
use rand::{Rng, RngExt};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec;

pub struct QuoteGenerator {
    quotes: HashMap<String, StockQuote>,
    price_deviation: u8,
    tick_duration: u64,
}

#[derive(Debug)]
enum QuoteGeneratorError {
    InvalidTicker,
}

impl QuoteGenerator {
    pub fn new(supported_tickers: Vec<&str>, price_deviation: u8, tick_duration: u64) -> Self {
        let mut quotes = HashMap::new();
        for ticker in supported_tickers {
            quotes.insert(
                ticker.to_string(),
                StockQuote {
                    ticker: ticker.to_string(),
                    price: rand::random::<f64>() * 1000.0,
                    volume: 0,
                    timestamp: 0,
                },
            );
        }
        Self {
            quotes,
            price_deviation,
            tick_duration,
        }
    }

    pub fn generate_quote(ticker: &str, last_price: f64, deviation_pt: u8) -> StockQuote {
        let volume = match ticker {
            // Популярные акции имеют больший объём
            "AAPL" | "MSFT" | "TSLA" => 1000 + (rand::random::<f64>() * 5000.0) as u32,
            // Обычные акции - средний объём
            _ => 100 + (rand::random::<f64>() * 1000.0) as u32,
        };

        StockQuote {
            ticker: ticker.to_string(),
            price: QuoteGenerator::next_tick_price(last_price, deviation_pt),
            volume,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }

    pub fn get_quotes(
        &mut self,
        tickers: Vec<&str>,
    ) -> Result<Vec<StockQuote>, QuoteGeneratorError> {
        let mut result = Vec::new();
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for ticker in tickers {
            let quote = self
                .quotes
                .get_mut(ticker)
                .ok_or(QuoteGeneratorError::InvalidTicker)?;

            if quote.timestamp + self.tick_duration < current_time {
                *quote = QuoteGenerator::generate_quote(ticker, quote.price, self.price_deviation);
            }

            result.push(quote.clone());
        }

        Ok(result)
    }

    fn next_tick_price(curent_price: f64, deviation_pt: u8) -> f64 {
        let mut rng = rand::rng();
        let deviation = curent_price * deviation_pt as f64 / 100.0;

        curent_price + rng.random_range(-deviation..deviation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_get_quotes_returns_correct_tickers() {
        let mut generator = QuoteGenerator::new(
            vec!["AAPL", "GOOG"],
            5,
            Duration::from_millis(1000).as_millis() as u64,
        );
        let quotes = generator.get_quotes(vec!["AAPL", "GOOG"]).unwrap();
        assert_eq!(quotes.len(), 2);
        for quote in &quotes {
            assert!(quote.price > 0.0 && quote.price < 1000.0);
        }
    }

    #[test]
    fn test_quote_refreshes_after_tick() {
        let mut generator = QuoteGenerator::new(
            vec!["AAPL"],
            5,
            Duration::from_millis(100).as_millis() as u64,
        );
        let q1 = generator.get_quotes(vec!["AAPL"]).unwrap()[0].price;
        let mut q2 = generator.get_quotes(vec!["AAPL"]).unwrap()[0].price;

        //price same
        assert!(q1 == q2);

        std::thread::sleep(Duration::from_millis(200));
        q2 = generator.get_quotes(vec!["AAPL"]).unwrap()[0].price;

        //price different
        assert!(q1 != q2);
    }

    #[test]
    fn test_invalid_ticker_returns_error() {
        let mut generator = QuoteGenerator::new(
            vec!["AAPL"],
            5,
            Duration::from_millis(1000).as_millis() as u64,
        );
        assert!(generator.get_quotes(vec!["INVALID"]).is_err());
    }
}
