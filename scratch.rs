use skim::prelude::*;
use std::io::Cursor;
fn main() {
    let options = SkimOptionsBuilder::default().build().unwrap();
    let items = "foo\nbar\nbaz";
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(items));
    let out = Skim::run_with(&options, Some(items)).unwrap();
    for item in out.selected_items {
        println!("{}", item.output());
    }
}
