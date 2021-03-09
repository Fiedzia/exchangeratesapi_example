pub fn reqwest_error_to_string(err: reqwest::Error) -> String {
    format!("{:?}", err)
}

pub fn json_error_to_string<T>(param: &T) -> String
    where T: core::fmt::Display 
{
    format!("cannot parse json: {}", param)
}

