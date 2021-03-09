/*

CLI for exchangeratesapi

USAGE:
    exchange <CURRENCYFROM> <CURRENCYTO> <DATEFROM> <DATETO>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <CURRENCYFROM>
    <CURRENCYTO>
    <DATEFROM>        date in format YYYY-MM-DD
    <DATETO>          date in format YYYY-MM-DD

obtained results will be cached in files in local directory
small percentage of requests is allowed to fail, you'll see notice about that
larget percentage or failures will show error
*/
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use chrono::Datelike;
use chrono::naive::NaiveDate;
use structopt::StructOpt;

mod types;
mod utils;

use types::{Opt, ExchangeValue, ExchangeProvider, ExchangeResult};
use utils::{reqwest_error_to_string, json_error_to_string};

//if we cannot retrieve rates for more than this fraction of working days, exit with an error,
//otherwise show notice, but produce result
const ACCEPTABLE_RETRIEVAL_FAILURE_FRACTION: f64 = 0.05;
const EXCHANGE_URL: &str = "https://api.exchangeratesapi.io/";

/*
 * Retrieve exchange rates from external service or cache if available,
 * cache returned value in local file
 */
pub fn get_exchange_rate(currency_from: &str, currency_to: &str, date: &NaiveDate) -> Result<f64, String> {

    let formatted_date:String = date.format("%F").to_string();
    //try to get cached results from a file
    let fname = format!("{}_{}_{}.cached", currency_from, currency_to, formatted_date);
    let path = Path::new(&fname);
    if path.exists() {
         let mut file = File::open(&path).map_err(|e| format!("cannot open cache file: {} {}",e ,fname))?;
         let mut buffer = Vec::new();
         file.read_to_end(&mut buffer).map_err(|e| format!("cannot read cache file: {} {}",e ,fname))?;
         match std::str::from_utf8(buffer.as_slice()) {
             Ok(txt) => {
                 match txt.parse::<f64>() {
                    Ok(value) => return Ok(value),
                    Err(e) => return Err(format!("cannot parse cached rate value: {} {}", fname, e))
                 }
             }
             Err(e) => return Err(format!("invalid cache file content: {} {}",fname, e))
         }
    }

    let client = reqwest::blocking::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(reqwest_error_to_string)?;
    let json_body: serde_json::Value = client
        .get(&format!("{}{}", &EXCHANGE_URL, &formatted_date))
        .query(&[
            ("symbols", format!("{},{}", currency_from, currency_to)),
            ("base", currency_from.to_string()),
        ])
        .send()
        .map_err(reqwest_error_to_string)?
        .json()
        .map_err(reqwest_error_to_string)?;


    //{"rates":{"USD":1.0,"GBP":0.7224675544},"base":"USD","date":"2021-03-08"} -> 0.7224675544
    let rate_value: f64 = json_body
        .as_object()
        .ok_or_else(|| json_error_to_string(&json_body))?
        .get("rates")
        .ok_or_else(|| json_error_to_string(&json_body))?
        .as_object()
        .ok_or_else(|| json_error_to_string(&json_body))?
        .get(currency_to)
        .ok_or_else(|| json_error_to_string(&json_body))?
        .as_f64()
        .ok_or_else(|| json_error_to_string(&json_body))?;

    //cache value
    let mut file = File::create(&path).map_err(|e| format!("cannot create cache file: {:?} {}", path, e))?;
    file.write_all(rate_value.to_string().as_bytes())
        .map_err(|e| format!("cannot write cache file: {:?} {}", path, e))?;

    Ok(rate_value)
}

/*
 *  Obtain exchange rates for given currencies and data range
 *  to make testing easier, exchanges are obtained via ExchangeProvider
 * */
pub fn exchange_rate_overview(opt: &Opt, exchange_provider: ExchangeProvider) -> ExchangeResult {
    if opt.currency_from.to_lowercase() == opt.currency_to.to_lowercase() {
        return Err("You have to pick two different currencies".to_string());
    }

    let date_diff = opt.date_to.signed_duration_since(opt.date_from);
    if date_diff.num_days() < 0 {
        return Err("date_from must precede or be equal to date_to".to_string());
    }
 
    let mut processed_date = opt.date_from;
    let mut expected_days = 0; // amount of Mon-Fri days we expect to get data for
    let mut retrieved_days = 0; // amount of days we retrieved data for

    let mut rate_sum: f64 = 0f64;
    let mut max_rate: Option<(f64, NaiveDate)> = None;
    let mut min_rate: Option<(f64, NaiveDate)> = None;


    while opt.date_to.signed_duration_since(processed_date).num_days() >= 0 {
        match processed_date.weekday() {
            chrono::Weekday::Sat | chrono::Weekday::Sun => {},
            _ => {
                expected_days += 1;
                match exchange_provider(&opt.currency_from, &opt.currency_to, &processed_date) {
                    Ok(value) => {
                        retrieved_days += 1;
                        rate_sum += value;
                        max_rate = match max_rate {
                            Some(v) => if value > v.0 {
                                Some((value, processed_date))
                            } else {
                                Some(v)
                            },
                            None => Some((value, processed_date))
                        };
                        min_rate = match min_rate {
                            Some(v) => if value < v.0 {
                                Some((value, processed_date))
                            } else {
                                Some(v)
                            },
                            None => Some((value, processed_date))
                        };
                    },
                    Err(err_str) => {
                        eprintln!("{} -> Failed to retrieve rates: {}", &processed_date, err_str);
                    }
                }
            }
        }

        processed_date += chrono::Duration::days(1);
    }

    if expected_days == 0 || retrieved_days == 0 {
        return Err("Could not retrieve even 1 rate. Perhaps pick a date range with more working days.".to_string());
    }

    if (1f64-(retrieved_days as f64/expected_days as f64)) > ACCEPTABLE_RETRIEVAL_FAILURE_FRACTION {
        return Err(format!("Failure rate exceeded acceptable threshold ({})", ACCEPTABLE_RETRIEVAL_FAILURE_FRACTION));
    }


    Ok(ExchangeValue {
        mean_rate: rate_sum as f64/retrieved_days as f64,
        min_rate: min_rate.unwrap(),
        max_rate: max_rate.unwrap(),
        notice: if expected_days == retrieved_days {
            None
        } else {
            Some(format!("we failed to retrieve {} of {} rates", expected_days-retrieved_days, expected_days))
        }
    })
}


fn main() {
    let opt = Opt::from_args();
    println!("For {}:{} between {} and {}", opt.currency_from, opt.currency_to, opt.date_from, opt.date_to);

    match exchange_rate_overview(&opt, get_exchange_rate) {
        Ok(exchange_result) => println!("{:#?}", exchange_result),
        Err(err_str) => {
            eprintln!("{}", err_str);
            std::process::exit(1);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exchange_rate_overview() {

        //simplest test for single date for working day
        let mut opt = Opt {
            currency_from: "AAA".to_string(),
            currency_to: "BBB".to_string(),
            date_from: NaiveDate::from_ymd(2021, 3, 1),
            date_to: NaiveDate::from_ymd(2021, 3, 1),
        };

        assert_eq!(
            exchange_rate_overview(&opt, |_, _, _| { Ok(1f64) }),
            Ok(ExchangeValue {
                mean_rate: 1f64,
                min_rate: (1f64, opt.date_from),
                max_rate: (1f64, opt.date_to), 
                notice: None
            })
        );

        //test that asking for results from Saturday when we wouldn't have any fails
        opt.date_from = NaiveDate::from_ymd(2021, 3, 6); //Sat
        opt.date_to = NaiveDate::from_ymd(2021, 3, 6); //Sat

        assert_eq!(
            exchange_rate_overview(&opt, |_, _, _| { Ok(1f64) }),
            Err("Could not retrieve even 1 rate. Perhaps pick a date range with more working days.".to_string())
        );

        //test that asking for 5 working days calculates mean/min/max correctly
        opt.date_from = NaiveDate::from_ymd(2021, 3, 1); //Sat
        opt.date_to = NaiveDate::from_ymd(2021, 3, 5); //Sat

        assert_eq!(
            //5 days, rates values are 1..5
            exchange_rate_overview(&opt, |_, _, date| { Ok(date.day() as f64) }),
            Ok(ExchangeValue {
                mean_rate: 3.0,
                min_rate: (1f64, opt.date_from),
                max_rate: (5f64, opt.date_to), 
                notice: None
            })
        );

        //test that asking for 46 working days when 1 fails shows notice
        opt.date_from = NaiveDate::from_ymd(2021, 1, 1);
        opt.date_to = NaiveDate::from_ymd(2021, 3, 5); 

        assert_eq!(
            //46 days, rates values 1..31, 1..28, 1..4, Error
            exchange_rate_overview(
                &opt,
                |_, _, date| {
                    if (date.month() == 3 && date.day() == 5) {
                        Err("error".to_string())
                    } else  {
                        Ok(date.day() as f64)
                    }
                }
            ),
            Ok(ExchangeValue {
                mean_rate: 13.577777777777778,
                min_rate: (1f64, opt.date_from),
                max_rate: (29f64, NaiveDate::from_ymd(2021, 1, 29)),
                notice: Some("we failed to retrieve 1 of 46 rates".to_string())
            })
        );


        //test that exceeding error margin fails
        opt.date_from = NaiveDate::from_ymd(2021, 1, 1);
        opt.date_to = NaiveDate::from_ymd(2021, 3, 5); 

        assert_eq!(
            //46 days, rates values 1..31, 1..28, 5*Error
            exchange_rate_overview(
                &opt,
                |_, _, date| {
                    if (date.month() == 3 ) {
                        Err("error".to_string())
                    } else  {
                        Ok(date.day() as f64)
                    }
                }
            ),
            Err(format!("Failure rate exceeded acceptable threshold ({})", ACCEPTABLE_RETRIEVAL_FAILURE_FRACTION))
        );
    }
}
