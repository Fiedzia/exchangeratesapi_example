use chrono::naive::NaiveDate;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "uppercase")]
#[structopt(about = "Use exchangeratesapi.io to get exchange rates for given time period")]
pub struct Opt {
    pub currency_from: String,
    pub currency_to: String,
    #[structopt(help = "date in format YYYY-MM-DD")]
    pub date_from: NaiveDate,
    #[structopt(help = "date in format YYYY-MM-DD")]
    pub date_to: NaiveDate,
}

#[derive(Debug, PartialEq)]
pub struct ExchangeValue {
    pub mean_rate: f64,
    pub min_rate: (f64, NaiveDate),
    pub max_rate: (f64, NaiveDate),
    pub notice: Option<String> //optional notice for users
}

pub type ExchangeResult = Result<ExchangeValue, String>;

pub type ExchangeProvider = fn (currency_from: &str, currency_to: &str, date: &NaiveDate) -> Result<f64, String>;
