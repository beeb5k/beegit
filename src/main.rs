pub mod beegit;
pub mod config;
mod git;

fn main() {
    let beegit = beegit::Beegit {
        config: config::Config::load(),
    };
}
