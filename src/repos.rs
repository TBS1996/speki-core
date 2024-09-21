use crate::config::Config;

pub fn pull() {
    let repos = Config::load().unwrap().repos;
}
