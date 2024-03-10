use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use prettytable::Row;
use regex::{NoExpand, Regex};
use sqlite::State;

use clap::{Parser, Subcommand};
#[macro_use]
extern crate prettytable;

static BKLIBRARY: &str =
    "/Library/Containers/com.apple.iBooksX/Data/Documents/BKLibrary/BKLibrary-1-091020131601.sqlite";
static AEANNOTATION: &str =
    "/Library/Containers/com.apple.iBooksX/Data/Documents/AEAnnotation/AEAnnotation_v10312011_1727_local.sqlite";

static SAVE_DIR: &str = "./bket";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Optional name to operate on
    name: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// show book list
    List,
    /// export highlight with asset id
    Export {
        asset_id: Option<String>,
        /// export all book's highlight
        #[arg(short, long)]
        all: bool,
    },
    /// returns result that include the text
    Search { text: String },
}

struct Library {
    pub asset_id: String,
    title: String,
    author: String,
    text: Vec<String>,
}

impl Library {
    fn save(&self) {
        if self.text.is_empty() {
            println!("the book's highlight is empty: {}", self.title);
            return;
        }
        if self.title.is_empty() {
            panic!("the save to filename is empty");
        }

        if check_filename(self.title.clone()) {
            // TODO check the file is exist
            println!("the file({}) is exist", self.title);
        }

        let mut content: Vec<String> = Vec::new();
        let re: Regex = Regex::new(r"\[[\d{1}|\d{2}]\]\s*").unwrap();
        let reg1 = Regex::new(r"^\(\d*\)$").unwrap();
        let re_comma = Regex::new(r"^[,|，|。|.|！|!].*?").unwrap();

        // ，这只是因为它们都处于相同的收缩空间之中。”"
        self.text.clone().into_iter().for_each(|mut ele| {
            if !ele.is_empty() {
                ele = re.replace_all(&ele, "").to_string();

                ele.split("\n").into_iter().for_each(|v| {
                    if !v.trim().is_empty() {
                        let mut str_reg1 = reg1.replace(v, "").to_string();
                        if str_reg1.trim().is_empty() {
                            return;
                        }
                        str_reg1 = re_comma.replace_all(&str_reg1.trim(), NoExpand("")).to_string();

                        content.push(format!("\"{}\"", str_reg1.trim().replace("\t", "")));
                    }
                });
            }
        });

        let filename = format!("{}/{}.md", SAVE_DIR, self.title);

        // check dir has exist
        if !check_dir_ok(SAVE_DIR) {
            create_dir(SAVE_DIR).unwrap();
        }

        let mut file = File::create(filename.clone()).unwrap();

        file.write_all(content.join("\n\n").as_bytes()).unwrap();

        file.flush().unwrap();
        // TODO input log to console
        println!("exported books: {}", filename);
    }
}

fn check_dir_ok(path: &str) -> bool {
    Path::new(path).is_dir()
}

fn create_dir(path: &str) -> Result<(), std::io::Error> {
    fs::create_dir(path)
}

struct Ibook {
    annotation: sqlite::Connection,
    library: sqlite::Connection,
}

impl Ibook {
    fn new() -> Self {
        Ibook {
            annotation: get_bk_ae_annotation(),
            library: get_bk_library(),
        }
    }

    fn query_annotation(&self, sql: &str) -> sqlite::Statement<'_> {
        return self.annotation.prepare(sql).unwrap();
    }

    fn query_library(&self, sql: &str) -> sqlite::Statement<'_> {
        return self.library.prepare(sql).unwrap();
    }

    fn get_library(&self) -> Vec<Library> {
        let sql = "SELECT * FROM ZBKLIBRARYASSET ORDER  BY Z_PK DESC;";
        self.get_library_with_sql(sql)
    }

    fn get_library_with_text(&self, text: String) -> Vec<Library> {
        let sql = format!(
            "SELECT * FROM ZBKLIBRARYASSET WHERE ZTITLE LIKE '%{}%' OR ZAUTHOR LIKE '%{}%'",
            text, text
        );
        self.get_library_with_sql(&sql)
    }

    fn get_library_with_sql(&self, sql: &str) -> Vec<Library> {
        let mut statement = self.query_library(sql);

        let mut result: Vec<Library> = Vec::new();
        while let Ok(State::Row) = statement.next() {
            let asset_id = statement.read::<String, _>("ZASSETID").unwrap_or_default();
            let library_item = self.get_library_with_asset_id(&asset_id).unwrap();
            result.push(library_item);
        }
        result
    }

    fn get_library_with_asset_id(&self, asset_id: &str) -> Option<Library> {
        let sql = format!(
            "SELECT * FROM ZBKLIBRARYASSET WHERE ZASSETID = '{}' AND ZTITLE != '' LIMIT 1",
            asset_id
        );
        let mut statement = self.query_library(&sql);

        while let Ok(State::Row) = statement.next() {
            // To get annotation with asset id.
            let text = self.get_annotation_with_asset_id(asset_id);

            let asset_id = statement.read::<String, _>("ZASSETID").unwrap_or_default();
            let title = statement.read::<String, _>("ZTITLE").unwrap_or_default();
            let author = statement.read::<String, _>("ZAUTHOR").unwrap_or_default();

            return Some(Library {
                asset_id: asset_id,
                title: title,
                author: author,
                text: text,
            });
        }
        None
    }

    fn get_annotation_with_asset_id(&self, asset_id: &str) -> Vec<String> {
        let sql =
            format!("SELECT * FROM ZAEANNOTATION WHERE ZANNOTATIONASSETID = '{}' AND ZANNOTATIONREPRESENTATIVETEXT != '';", asset_id);

        let mut statement = self.query_annotation(&sql);
        let mut highlight: Vec<String> = Vec::new();
        while let Ok(State::Row) = statement.next() {
            let text = statement
                .read::<String, _>("ZANNOTATIONREPRESENTATIVETEXT")
                .unwrap_or_default();
            highlight.push(text);
        }
        highlight
    }
}

fn print_library(libraries: Vec<Library>) {
    let mut table: Vec<Vec<String>> = vec![vec![
        "AssetID".to_string(),
        "Title".to_string(),
        "Author".to_string(),
        "Count".to_string(),
    ]];

    libraries.into_iter().for_each(|x| {
        table.push(vec![
            x.asset_id,
            ellipsis_text(x.title),
            ellipsis_text(x.author),
            x.text.len().to_string(),
        ]);
    });

    table_print(table)
}

// connect sqlite with sql's path
fn connect_sqlite(path: String) -> sqlite::Connection {
    sqlite::open(format!("{}{}", get_home_dir(), path)).unwrap()
}

fn table_print(content: Vec<Vec<String>>) {
    let mut table: prettytable::Table = table!();
    for row in content {
        table.add_row(Row::from(row));
    }

    table.printstd();
}

fn ellipsis_text(mut text: String) -> String {
    if text.starts_with(",") {
        let re_comma = Regex::new(r"^[,|，|。|.|！|!].*").unwrap();
        text = re_comma.replace(&text, NoExpand("")).to_string();
    }

    let braces = Regex::new(r"[（|()].*?[）|)]").unwrap();
    let replaced = braces.replace_all(&text, NoExpand(""));
    text = replaced.to_string();

    let te: Vec<_> = text.chars().collect();

    if te.len() < 20 {
        return text.trim().to_string();
    }
    text = te[0..20].iter().collect();

    return text.trim().to_string();
}

fn check_filename(filename: String) -> bool {
    Path::new(filename.as_str()).is_file()
}

fn get_bk_library() -> sqlite::Connection {
    return connect_sqlite(BKLIBRARY.to_string());
}

fn get_bk_ae_annotation() -> sqlite::Connection {
    return connect_sqlite(AEANNOTATION.to_string());
}

fn ibook_cli() {
    let cli = Cli::parse();
    let book = Ibook::new();

    // You can check the value provided by positional arguments, or option arguments
    if let Some(name) = cli.name.as_deref() {
        println!("Value for name: {name}");
    }

    match &cli.command {
        Some(commands) => match commands {
            Commands::List => {
                let libraries = book.get_library();
                print_library(libraries)
            }
            Commands::Export { asset_id, all } => {
                if *all {
                    book.get_library().into_iter().for_each(|x| x.save())
                } else {
                    match book.get_library_with_asset_id(asset_id.as_ref().unwrap().as_str()) {
                        Some(v) => v.save(),
                        None => {
                            println!(
                                "Don't find highlight of the asset_id[{}]",
                                asset_id.clone().unwrap()
                            )
                        }
                    }
                }
            }
            Commands::Search { text } => {
                print_library(book.get_library_with_text(text.to_string()))
            }
        },
        None => todo!(),
    }
}

// get home directory
fn get_home_dir() -> String {
    dirs::home_dir().unwrap().to_str().unwrap().to_string()
}

fn main() {
    if std::env::consts::OS != "macos" {
        panic!(
            "Don't supported platform[{}] and please running on macos, try it again!",
            std::env::consts::OS
        );
    }
    ibook_cli()
}

#[cfg(test)]
mod tests {
    use crate::{print_library, Ibook};

    #[test]
    fn list() {
        let book = Ibook::new();
        let library = book.get_library();
        print_library(library);
    }

    #[test]
    fn search() {
        let book = Ibook::new();
        let library = book.get_library_with_text(String::from("从一"));
        print_library(library);
    }

    #[test]
    fn export() {
        let book = Ibook::new();
        let libraries = book.get_library_with_text(String::from("超越"));
        libraries.into_iter().for_each(|x| x.save())
    }

    #[test]
    fn export_with_asset_id() {
        let book = Ibook::new();
        book.get_library_with_asset_id("1E066462EBBBAC91B95533B1426531B0")
            .into_iter()
            .for_each(|x| x.save());
    }

    #[test]
    fn export_all() {
        let book = Ibook::new();
        book.get_library().into_iter().for_each(|x| x.save());
    }
}
