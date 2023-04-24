use std::fs::File;
use std::io::{prelude::*, BufReader};

/// Source object is represented here
pub struct Source {
    url: String,
    path: String,
    params: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
}

impl Source { 
    pub fn new<S: Into<String>>(url: S, path: S, params: Vec<(String, String)>, headers: Vec<(String, String)>) -> Source {
        Source {
            url: url.into(),
            path: path.into(),
            params: params,
            headers: headers,
        }
    }

    pub fn params(&self) -> &Vec<(String, String)> {
        &self.params
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn headers(&self) -> &Vec<(String, String)> {
        &self.headers
    }
}

pub fn get_rand_source(query_vals: Vec<String>) -> Source {
    let file = File::open("urls.txt")
        .expect("Problem while opening sources file");

    let buf_reader = BufReader::new(file);

    let lines: Vec<String> = buf_reader
        .lines()
        .map(|x| x.expect("Problem while reading file"))
        .collect();
    
    let line: Vec<String> = lines[0].split("; ").map(|x| x.to_owned()).collect();

    let mut url:String = Default::default();
    let mut path: String = Default::default();
    let mut params: Vec<(String, String)> = Vec::new();
    let mut headers: Vec<(String, String)> = Vec::new();

    for part in line {
        let p: Vec<String> = part.split(": ").map(|x| x.to_owned()).collect();

        match p[0].as_str()  {
            "url_base" => url = p[1].to_owned(),
            "path" => path = p[1].to_owned(),
            "params" => {
                let mut iter = 0;
                for param in p[1].split(' ').map(|x| x.to_owned()).collect::<Vec<String>>() {
                    if param.contains('=') {
                        let pair: Vec<&str> = param.split('=').collect();
                        params.push((pair[0].to_owned(), pair[1].to_owned()));
                    } else {
                        params.push((param.to_owned(), query_vals[iter].to_owned()));
                    }

                    iter += 1;
                }
            },
            "headers" => {
                for header in p[1].split(' ').map(|x| x.to_owned()).collect::<Vec<String>>() {
                    let pair: Vec<&str> = header.split('=').collect();
                    headers.push((pair[0].to_owned(), pair[1].to_owned()));
                }
            },
            _ => panic!("Unknown symbol in url"),
        }
    }

    Source::new(url, path, params, headers)
}
