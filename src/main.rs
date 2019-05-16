use coral::*;

fn main() {
    for entry in Analyzer::new().unwrap() {
        println!("{:#?}", entry);
    }
}
