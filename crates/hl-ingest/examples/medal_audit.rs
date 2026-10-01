#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db = hl_db::Db::connect(std::env::args().nth(1).unwrap()).await?;
    let teams = db.etf2l_teams().await?;
    let cat = hl_ingest::catalogue::Catalogue::load(&db).await?;
    for ((season, division), list) in cat.medals() {
        let names: Vec<String> = list.iter().map(|(p, t, how)| format!("{p}:{} [{how}]", teams.get(t).map_or("?", |x| x.0.as_str()))).collect();
        println!("S{season} {division}: {}", names.join(" | "));
    }
    Ok(())
}
