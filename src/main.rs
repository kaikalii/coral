use coral::*;

fn main() {
    for message in parse()
        .into_iter()
        .filter(|entry| entry.reason == Reason::CompilerMessage)
    {
        println!("{:#?}", message);
    }
}
